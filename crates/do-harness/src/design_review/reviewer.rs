//! Fresh reviewer launch, outcome classification, and sanitized recording.
//!
//! The reviewer is a separate process that receives exactly one packet JSON
//! object on stdin and returns a findings JSON object on stdout. The process
//! inherits a scrubbed environment (no session, agent, credential, or tool
//! state) and a fresh, empty conversation: the adapter passes nothing but
//! the packet. Native launch records (spawn time, command hash, scrubbed
//! environment key list) are recorded as process-isolation evidence.
//!
//! Outcomes are distinct and exhaustive: `success_with_findings`,
//! `success_zero_findings`, `failure` (launch/parse/reviewer error or marker
//! leak), `timeout` (deadline exceeded, killed), `not_run` (capability
//! unavailable), and `unverified` (completed but isolation evidence
//! incomplete). An unavailable or unverified capability is never a success.

use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::Result;
use serde_json::{Value, json};

use super::packet::{canonical_json, sha256_hex};

/// Stderr marker with which a reviewer declares that it has no capability;
/// classified as `not_run`, never a success.
pub const UNAVAILABLE_REASON: &str = "capability unavailable";
/// Exit code meaning "capability unavailable" in the reviewer contract.
pub const UNAVAILABLE_EXIT_CODE: i32 = 9;
/// Poll cadence while waiting for a reviewer process to finish.
const WAIT_POLL_INTERVAL: Duration = Duration::from_millis(50);

/// Environment keys a fresh reviewer may inherit. Everything else — session,
/// agent, credential, and tool state — is excluded so the child cannot read
/// parent conversation or connection context from the environment.
pub const CLEAN_ENV_ALLOW: &[&str] = &[
    "PATH",
    "HOME",
    "LANG",
    "LC_ALL",
    "TMPDIR",
    "TEMP",
    "TMP",
    "SYSTEMROOT",
    "SystemRoot",
];

/// The scrubbed environment a reviewer process inherits.
#[must_use]
pub fn clean_env() -> BTreeMap<String, String> {
    CLEAN_ENV_ALLOW
        .iter()
        .filter_map(|key| {
            let value = std::env::var(key).ok()?;
            (!value.is_empty()).then(|| ((*key).to_owned(), value))
        })
        .collect()
}

/// Distinct review outcomes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// Reviewer completed within limits; 1..=max findings.
    SuccessWithFindings,
    /// Reviewer completed within limits; zero findings.
    SuccessZeroFindings,
    /// Launch/parse/reviewer error, or isolation marker leaked.
    Failure,
    /// Reviewer exceeded the predeclared deadline and was killed.
    Timeout,
    /// Capability unavailable (no reviewer configured, or the reviewer
    /// declared itself unavailable).
    NotRun,
    /// Review completed but isolation evidence is incomplete.
    Unverified,
}

impl Outcome {
    /// Wire name of the outcome, stable in records and documentation.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::SuccessWithFindings => "success_with_findings",
            Self::SuccessZeroFindings => "success_zero_findings",
            Self::Failure => "failure",
            Self::Timeout => "timeout",
            Self::NotRun => "not_run",
            Self::Unverified => "unverified",
        }
    }
}

/// Parent-only marker probe over captured reviewer output.
///
/// `Some(true)` when the marker appears (isolation violated), `Some(false)`
/// when a marker was configured and did not appear, `None` when no marker
/// was configured (probe not run).
#[must_use]
pub fn probe_marker(output: &str, marker: Option<&str>) -> Option<bool> {
    marker.map(|marker| output.contains(marker))
}

/// Required non-empty string fields of a finding.
const FINDING_REQUIRED_FIELDS: &[&str] = &[
    "contract_or_path",
    "trigger",
    "consequence",
    "evidence",
    "smallest_correction",
];

/// Validated, capped findings from a reviewer response.
struct ValidatedFindings {
    findings: Vec<Value>,
    truncated: bool,
}

/// Validates and caps a reviewer's findings list.
///
/// # Errors
///
/// Fails when the response is not an object with a `findings` array, or a
/// finding is missing a required non-empty string field — invalid records
/// are a failure, never a silent drop.
fn validate_findings(payload: &Value, max_findings: usize) -> Result<ValidatedFindings> {
    let Some(items) = payload.get("findings").and_then(Value::as_array) else {
        anyhow::bail!("reviewer response must be an object with a findings array");
    };
    let mut valid = Vec::new();
    for (index, item) in items.iter().enumerate() {
        if !item.is_object() {
            anyhow::bail!("finding {index} is not an object");
        }
        let missing: Vec<&str> = FINDING_REQUIRED_FIELDS
            .iter()
            .filter(|field| {
                item.get(**field)
                    .and_then(Value::as_str)
                    .is_none_or(|value| value.trim().is_empty())
            })
            .copied()
            .collect();
        if !missing.is_empty() {
            anyhow::bail!("finding {index} missing fields: {}", missing.join(", "));
        }
        valid.push(item.clone());
    }
    Ok(ValidatedFindings {
        truncated: valid.len() > max_findings,
        findings: valid.into_iter().take(max_findings).collect(),
    })
}

/// Bounded capture: `max_output_bytes` plus one byte, so an oversized
/// response stays detectable after capping.
fn spawn_bounded_reader(
    mut reader: impl Read + Send + 'static,
    max_output_bytes: usize,
) -> std::thread::JoinHandle<Vec<u8>> {
    std::thread::spawn(move || {
        let cap = max_output_bytes.saturating_add(1);
        let mut buffer = vec![0_u8; 4096];
        let mut captured = Vec::new();
        loop {
            if captured.len() >= cap {
                break;
            }
            match reader.read(&mut buffer) {
                Ok(0) | Err(_) => break,
                Ok(read) => {
                    let room = cap - captured.len();
                    captured.extend_from_slice(&buffer[..read.min(room)]);
                }
            }
        }
        captured
    })
}

fn spawn_time_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs())
}

/// Launch outcome of [`run_reviewer`]'s child process.
enum WaitResult {
    /// The process exited with this code.
    Exited(i32),
    /// The deadline passed first; the child was killed.
    Killed,
}

/// Waits for the child until the deadline, then kills it.
fn wait_with_deadline(child: &mut Child, deadline: Instant) -> WaitResult {
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                return WaitResult::Exited(status.code().unwrap_or(-1));
            }
            Ok(None) => {
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    return WaitResult::Killed;
                }
                std::thread::sleep(WAIT_POLL_INTERVAL);
            }
            Err(_) => return WaitResult::Exited(-1),
        }
    }
}

/// Launch the fresh reviewer, capture sanitized evidence, classify outcome.
///
/// Returns a record with fixed reason codes and structured findings; raw
/// reviewer stdout/stderr never enter the record.
///
/// # Errors
///
/// Propagates only a reviewer-command canonicalization failure; every
/// reviewer behavior — including launch failure, timeout, marker leak, and
/// unparsable output — is a recorded outcome, not an error.
#[allow(clippy::too_many_lines)]
pub fn run_reviewer(
    cmd: &[String],
    packet_json: &str,
    repo: &Path,
    timeout_s: u64,
    max_output_bytes: usize,
    max_findings: usize,
    marker: Option<&str>,
) -> Result<Value> {
    let spawn_time = spawn_time_unix();
    let started = Instant::now();
    let env = clean_env();
    let env_keys: Vec<String> = env.keys().cloned().collect();

    let mut record = json!({
        "reviewer_cmd_hash": sha256_hex(&canonical_json(&json!(cmd))?),
        "launch_evidence": {
            "fresh_process": true,
            "spawn_time_unix": spawn_time,
            "environment_scrubbed": true,
            "environment_keys_inherited": env_keys,
        },
        "limits": {
            "timeout_s": timeout_s,
            "max_output_bytes": max_output_bytes,
            "max_findings": max_findings,
        },
    });

    let mut command = Command::new(&cmd[0]);
    command
        .args(&cmd[1..])
        .current_dir(repo)
        .env_clear()
        .envs(&env)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let Ok(mut child) = command.spawn() else {
        record["launch_evidence"]["fresh_process"] = Value::Bool(false);
        record["outcome"] = json!(Outcome::Failure.as_str());
        record["reason"] = json!("reviewer could not start");
        record["error_kind"] = json!("reviewer_launch");
        return Ok(record);
    };

    // The packet is the reviewer's only input: write it to stdin and close.
    let packet_input = packet_json.to_owned();
    if let Some(mut stdin) = child.stdin.take() {
        std::thread::spawn(move || {
            let _ = stdin.write_all(packet_input.as_bytes());
        });
    }

    let stdout_reader = child.stdout.take();
    let stderr_reader = child.stderr.take();
    let stdout_handle = stdout_reader.map_or_else(
        || std::thread::spawn(Vec::new),
        |reader| spawn_bounded_reader(reader, max_output_bytes),
    );
    let stderr_handle = stderr_reader.map_or_else(
        || std::thread::spawn(Vec::new),
        |reader| spawn_bounded_reader(reader, max_output_bytes),
    );

    let deadline = started + Duration::from_secs(timeout_s.max(1));
    let wait = wait_with_deadline(&mut child, deadline);
    let stdout = stdout_handle.join().unwrap_or_default();
    let stderr = stderr_handle.join().unwrap_or_default();
    let elapsed = started.elapsed().as_secs_f64();
    record["elapsed_seconds"] = json!((elapsed * 1000.0).round() / 1000.0);

    let decode = |bytes: &[u8]| String::from_utf8_lossy(bytes).into_owned();

    match wait {
        WaitResult::Killed => {
            record["outcome"] = json!(Outcome::Timeout.as_str());
            record["reason"] = json!("reviewer exceeded the predeclared timeout");
            let partial = decode(&stdout);
            let partial_err = decode(&stderr);
            record["marker_leaked"] = json!(probe_marker(&(partial + &partial_err), marker));
            Ok(record)
        }
        WaitResult::Exited(code) => {
            let output = decode(&stdout);
            let err = decode(&stderr);
            record["reviewer_exit_code"] = json!(code);
            record["marker_leaked"] = json!(probe_marker(&(output.clone() + &err), marker));

            if code == UNAVAILABLE_EXIT_CODE && err.to_lowercase().contains(UNAVAILABLE_REASON) {
                record["outcome"] = json!(Outcome::NotRun.as_str());
                record["reason"] = json!(format!(
                    "{UNAVAILABLE_REASON}: reviewer declared no capability"
                ));
                return Ok(record);
            }
            if code != 0 {
                record["outcome"] = json!(Outcome::Failure.as_str());
                record["reason"] = json!("reviewer exited nonzero");
                record["error_kind"] = json!("reviewer_exit");
                return Ok(record);
            }
            if stdout.len() > max_output_bytes {
                record["outcome"] = json!(Outcome::Failure.as_str());
                record["reason"] = json!("reviewer response exceeds the output limit");
                return Ok(record);
            }
            let Ok(payload) = serde_json::from_str::<Value>(&output) else {
                record["outcome"] = json!(Outcome::Failure.as_str());
                record["reason"] = json!("reviewer stdout is not valid JSON");
                return Ok(record);
            };
            let validated = match validate_findings(&payload, max_findings) {
                Ok(validated) => validated,
                Err(error) => {
                    record["outcome"] = json!(Outcome::Failure.as_str());
                    record["reason"] = json!(error.to_string());
                    return Ok(record);
                }
            };
            record["findings"] = json!(validated.findings);
            record["findings_count"] = json!(validated.findings.len());
            record["findings_truncated"] = json!(validated.truncated);
            let outcome = if validated.findings.is_empty() {
                Outcome::SuccessZeroFindings
            } else {
                Outcome::SuccessWithFindings
            };
            record["outcome"] = json!(outcome.as_str());
            if record["marker_leaked"] == Value::Bool(true) {
                record["outcome"] = json!(Outcome::Failure.as_str());
                record["reason"] = json!("isolation marker leaked into reviewer output");
                record["error_kind"] = json!("isolation_marker_leaked");
            }
            Ok(record)
        }
    }
}
