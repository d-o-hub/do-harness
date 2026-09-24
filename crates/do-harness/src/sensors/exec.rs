//! Single-sensor process execution for `do-harness verify`.
//!
//! Every attempt goes through one spawn-and-poll path: the child is spawned
//! with piped stdio, pump threads drain both pipes, and a poll loop watches
//! for exit, timeout expiry, and fail-fast cancellation. A shared cancel flag
//! lets the parallel driver kill in-flight siblings; the flag is never set on
//! sequential non-fail-fast runs, so that path is unaffected.

use std::ffi::OsString;
use std::io::Read;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use crate::baselines::Baselines;
use crate::config::{SensorSeverity, SensorSpec};
use crate::report::SensorResult;

/// Poll interval for child exit, timeout, and cancel checks.
const POLL_INTERVAL: Duration = Duration::from_millis(10);

/// Elapsed milliseconds since `start`, saturating at `u64::MAX`.
fn elapsed_ms(start: Instant) -> u64 {
    u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX)
}

/// Parses the last `FINDINGS: <n>` marker in captured output, when present.
fn parse_findings(output: &str) -> Option<u64> {
    output.lines().rev().find_map(|line| {
        line.trim_start()
            .strip_prefix("FINDINGS:")
            .and_then(|rest| rest.trim().parse::<u64>().ok())
    })
}

/// Builds a [`SensorResult`] for `spec`, deriving `warned` from a passing run
/// whose output carries a `SKIP:` marker (a tool or runtime was unavailable)
/// and `findings` from the last `FINDINGS: <n>` marker.
pub(crate) fn sensor_result(
    spec: &SensorSpec,
    ok: bool,
    exit_code: Option<i32>,
    duration_ms: u64,
    output: String,
) -> SensorResult {
    let severity = spec.effective_severity();
    let warned = ok
        && output
            .lines()
            .any(|line| line.trim_start().starts_with("SKIP:"));
    SensorResult {
        name: spec.name.clone(),
        ok,
        exit_code,
        duration_ms,
        severity,
        allow_failure: severity == SensorSeverity::Warn,
        warned,
        findings: parse_findings(&output),
        baseline: None,
        output,
    }
}

/// Applies the blessed findings ratchet to a completed result.
///
/// With a baseline for the sensor: a count above the baseline is a hard
/// regression (fails even warn-severity sensors), a count within the
/// baseline is advisory, and zero findings change nothing. Without a
/// baseline the exit code and configured severity decide.
fn apply_ratchet(
    mut result: SensorResult,
    spec: &SensorSpec,
    baselines: &Baselines,
) -> SensorResult {
    let Some(findings) = result.findings else {
        return result;
    };
    let Some(baseline) = baselines.get(&spec.name) else {
        return result;
    };
    result.baseline = Some(baseline);
    if findings > baseline {
        result.ok = false;
        result.allow_failure = false;
        result.warned = false;
        result.output = format!(
            "{}\nratchet regression: findings {findings} exceed blessed baseline {baseline}",
            result.output
        );
    } else if findings > 0 {
        result.allow_failure = true;
        result.warned = true;
    }
    result
}

/// Environment variables that describe the crate which launched the harness,
/// rather than the sensor being executed.
const LAUNCHER_CRATE_VARS: [&str; 6] = [
    "CARGO_MANIFEST_DIR",
    "CARGO_MANIFEST_PATH",
    "CARGO_CRATE_NAME",
    "CARGO_BIN_NAME",
    "CARGO_PRIMARY_PACKAGE",
    "OUT_DIR",
];

/// Returns whether a key identifies the harness launcher crate.
///
/// Cargo-machete uses CARGO plus the absence of `CARGO_PKG_NAME` to detect
/// Cargo's external-subcommand dispatch. If verify itself was launched by
/// cargo run, the inherited `CARGO_PKG_NAME` makes cargo machete treat its
/// dispatch token (machete) as a directory. Sensors are independent tools,
/// so launcher crate identity must not leak into their environments.
fn is_launcher_crate_var(key: &str) -> bool {
    key.starts_with("CARGO_PKG_") || LAUNCHER_CRATE_VARS.contains(&key)
}

/// Removes launcher crate identity from a sensor command while preserving
/// toolchain and harness contract variables.
fn strip_launcher_crate_env<I>(command: &mut Command, variables: I)
where
    I: IntoIterator<Item = (OsString, OsString)>,
{
    for (key, _) in variables {
        if key.to_str().is_some_and(is_launcher_crate_var) {
            command.env_remove(key);
        }
    }
}

/// Spawns the sensor command with piped stdio, reporting a spawn failure as
/// a failed `SensorResult` instead of an error.
///
/// `bash`/`sh` programs are resolved through [`crate::shell`] because a bare
/// `bash` on Windows resolves to the WSL launcher in `System32` rather than Git
/// Bash: with no distribution installed it exits non-zero with an empty
/// diagnostic, so a stock rust-pack sensor (`["bash", "scripts/..."]`) would
/// fail confusingly on every Windows machine. Other programs are spawned as
/// declared.
fn spawn_sensor(
    spec: &SensorSpec,
    root: &Path,
    program: &str,
    rest: &[String],
    start: Instant,
) -> Result<Child, SensorResult> {
    let mut command = crate::shell::command(program, rest);
    command
        .current_dir(root)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    strip_launcher_crate_env(&mut command, std::env::vars_os());
    command.spawn().map_err(|err| {
        sensor_result(
            spec,
            false,
            None,
            elapsed_ms(start),
            format!("failed to spawn {program}: {err}"),
        )
    })
}

/// Spawns a thread that drains one piped stream to EOF.
fn pump(mut pipe: impl Read + Send + 'static) -> thread::JoinHandle<Vec<u8>> {
    thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = pipe.read_to_end(&mut buf);
        buf
    })
}

/// Joins the pump threads and concatenates stdout then stderr.
fn drain_pipes(
    stdout: Option<thread::JoinHandle<Vec<u8>>>,
    stderr: Option<thread::JoinHandle<Vec<u8>>>,
) -> String {
    let stdout = stdout.and_then(|h| h.join().ok()).unwrap_or_default();
    let stderr = stderr.and_then(|h| h.join().ok()).unwrap_or_default();
    format!(
        "{}\n{}",
        String::from_utf8_lossy(&stdout),
        String::from_utf8_lossy(&stderr)
    )
}

/// Kills and reaps the child, returning a failed result with `message`.
fn kill_and_report(
    child: &mut Child,
    spec: &SensorSpec,
    start: Instant,
    message: String,
) -> SensorResult {
    crate::shell::kill_tree(child);
    sensor_result(spec, false, None, elapsed_ms(start), message)
}

/// Runs a single sensor attempt from `root`, enforcing the spec timeout and
/// the shared fail-fast cancel flag.
///
/// A set cancel flag kills the child and reports a non-pass `cancelled`
/// result; timeout expiry kills the child and reports a non-pass timeout.
pub(crate) fn run_sensor_attempt(
    spec: &SensorSpec,
    root: &Path,
    cancel: &AtomicBool,
) -> SensorResult {
    let start = Instant::now();
    let Some((program, rest)) = spec.argv.split_first() else {
        return sensor_result(
            spec,
            false,
            None,
            0,
            format!("sensor '{}' has an empty argv", spec.name),
        );
    };
    let timeout = spec.timeout.map(Duration::from_secs);

    let mut child = match spawn_sensor(spec, root, program, rest, start) {
        Ok(child) => child,
        Err(result) => return result,
    };
    let stdout = child.stdout.take().map(pump);
    let stderr = child.stderr.take().map(pump);

    loop {
        if cancel.load(Ordering::SeqCst) {
            return kill_and_report(
                &mut child,
                spec,
                start,
                format!(
                    "sensor '{}' cancelled: fail-fast stopped this run",
                    spec.name
                ),
            );
        }
        match child.try_wait() {
            Ok(Some(status)) => {
                let output = drain_pipes(stdout, stderr);
                return sensor_result(
                    spec,
                    status.success(),
                    status.code(),
                    elapsed_ms(start),
                    output,
                );
            }
            Ok(None) => {
                if timeout.is_some_and(|budget| start.elapsed() >= budget) {
                    let secs = timeout.map_or(0, |budget| budget.as_secs());
                    return kill_and_report(
                        &mut child,
                        spec,
                        start,
                        format!("sensor '{}' timed out after {secs}s", spec.name),
                    );
                }
                thread::sleep(POLL_INTERVAL);
            }
            Err(err) => {
                return kill_and_report(
                    &mut child,
                    spec,
                    start,
                    format!("error waiting for child {program}: {err}"),
                );
            }
        }
    }
}

/// Runs a single sensor command from `root`, retrying on transient failures
/// and applying the blessed findings ratchet to the final result.
///
/// A set cancel flag stops further attempts after the attempt in flight.
pub(crate) fn run_sensor(
    spec: &SensorSpec,
    root: &Path,
    cancel: &AtomicBool,
    baselines: &Baselines,
) -> SensorResult {
    let max_retries = spec
        .retry
        .unwrap_or(if spec.transient_exit_codes.is_empty() {
            0
        } else {
            3
        });

    let mut attempts = 0;
    loop {
        let result = run_sensor_attempt(spec, root, cancel);
        if result.ok {
            return apply_ratchet(result, spec, baselines);
        }

        if attempts >= max_retries || (attempts > 0 && cancel.load(Ordering::SeqCst)) {
            return apply_ratchet(result, spec, baselines);
        }

        if !spec.transient_exit_codes.is_empty() {
            let is_transient = result
                .exit_code
                .is_some_and(|code| spec.transient_exit_codes.contains(&code));
            if !is_transient {
                return apply_ratchet(result, spec, baselines);
            }
        }

        attempts += 1;
        thread::sleep(Duration::from_millis(50));
    }
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;
    use std::process::Command;

    use super::{is_launcher_crate_var, strip_launcher_crate_env};

    #[test]
    fn recognizes_launcher_crate_identity_without_matching_contract_vars() {
        for key in [
            "CARGO_PKG_NAME",
            "CARGO_PKG_VERSION",
            "CARGO_MANIFEST_DIR",
            "CARGO_MANIFEST_PATH",
            "CARGO_CRATE_NAME",
            "CARGO_BIN_NAME",
            "CARGO_PRIMARY_PACKAGE",
            "OUT_DIR",
        ] {
            assert!(is_launcher_crate_var(key), "expected launcher key: {key}");
        }

        for key in [
            "CARGO",
            "CARGO_HOME",
            "CARGO_PKG",
            "CARGO_PKGX",
            "DO_HARNESS_REQUIRE_TOOLS",
            "PATH",
        ] {
            assert!(
                !is_launcher_crate_var(key),
                "unexpected launcher key: {key}"
            );
        }
    }

    #[test]
    fn strips_launcher_identity_and_preserves_sensor_contract_vars() {
        let mut command = Command::new("true");
        strip_launcher_crate_env(
            &mut command,
            [
                (
                    OsString::from("CARGO_PKG_NAME"),
                    OsString::from("do-harness"),
                ),
                (
                    OsString::from("CARGO_MANIFEST_DIR"),
                    OsString::from("/launcher"),
                ),
                (
                    OsString::from("DO_HARNESS_REQUIRE_TOOLS"),
                    OsString::from("1"),
                ),
                (OsString::from("PATH"), OsString::from("/usr/bin")),
            ],
        );

        let mut removed: Vec<_> = command
            .get_envs()
            .filter(|(_, value)| value.is_none())
            .map(|(key, _)| key.to_string_lossy().into_owned())
            .collect();
        removed.sort_unstable();
        assert_eq!(removed, ["CARGO_MANIFEST_DIR", "CARGO_PKG_NAME"]);
    }
}
