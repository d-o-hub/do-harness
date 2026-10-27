//! Optional persistence of verify runs into the agent-state database.

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::Result;

use crate::report::VerifyReport;

/// Maximum characters of a failing sensor's output stored in an error
/// signature message.
const MAX_SIGNATURE_MESSAGE: usize = 2_000;
const FAILURE_CONTEXT_CHARS: usize = 1_200;
const FINAL_OUTPUT_CHARS: usize = 500;

/// Consecutive failures after which `verify --record` halts a sensor.
pub const FAIL_FAST_STRIKES: i64 = 3;

/// Returns the sensor names whose `sensor:<name>` error signature for `task_id`
/// has `attempt_count >= FAIL_FAST_STRIKES`.
///
/// The caller partitions the result by severity: error-severity sensors are
/// halted, warn-severity sensors are quarantined.
///
/// # Errors
///
/// Returns an error when the state database cannot be initialized or queried.
pub async fn struck_sensors(root: &Path, names: &[String], scope: &str) -> Result<Vec<String>> {
    let conn = do_harness_db::connect_and_migrate(root).await?;
    let struck =
        do_harness_db::struck_signatures(&conn, scope, "sensor:", FAIL_FAST_STRIKES).await?;
    let struck: std::collections::HashSet<&str> = struck
        .iter()
        .filter_map(|signature| signature.strip_prefix("sensor:"))
        .collect();
    Ok(names
        .iter()
        .filter(|name| struck.contains(name.as_str()))
        .cloned()
        .collect())
}
/// Workstream scope for persisted beats (shared vocabulary from
/// `do-harness-types`, so beats and strikes cannot drift on the key).
pub use do_harness_types::BeatScope;

/// Resolves the effective beat scope from CLI flags and git state.
///
/// `task` is the raw `--task` value: `global` and a missing value fall back to
/// the current branch (or `global` outside a repository), a numeric value
/// scopes to that task id, and anything else is treated as an explicit branch
/// name.
#[must_use]
pub fn resolve_scope(root: &Path, task: Option<&str>, global: bool) -> BeatScope {
    if global {
        return BeatScope::Global;
    }
    if let Some(t) = task {
        if t == "global" {
            return BeatScope::Global;
        }
        if let Ok(id) = t.parse::<i64>() {
            return BeatScope::Task(id);
        }
        return BeatScope::Branch(t.to_owned());
    }
    if let Some(branch) = crate::changes::current_branch(root) {
        BeatScope::Branch(branch)
    } else {
        BeatScope::Global
    }
}

/// Records each sensor result atomically, scoped to `scope`: the beat and
/// its error-signature update (bump on failure, reset on pass) commit in one
/// transaction, and any observed findings count is upserted. Skipped sensors
/// (halted or quarantined) and results that did not execute in this run
/// (reused or cancelled) are omitted, so reuse can never look like a fresh
/// observation.
///
/// A clean pass whose declared input identity is unchanged across execution
/// (`pre_digests` was captured before the run and still matches now) stores
/// that identity on the beat, making the pass eligible for a later
/// `--unchanged=skip` reuse.
///
/// # Errors
///
/// Returns an error when the state database cannot be initialized or written.
pub async fn record_verify(
    root: &Path,
    report: &VerifyReport,
    skipped: &[String],
    scope: &BeatScope,
    cfg: &crate::config::Config,
    config_bytes: Option<&[u8]>,
    pre_digests: &BTreeMap<String, String>,
) -> Result<()> {
    let conn = do_harness_db::connect_and_migrate(root).await?;
    let specs: BTreeMap<&str, &crate::config::SensorSpec> = cfg
        .effective_sensors()
        .iter()
        .map(|spec| (spec.name.as_str(), spec))
        .collect();
    let now = do_harness_db::unix_now();
    let task_id = scope.task_id();
    let messages: Vec<String> = report
        .sensors
        .iter()
        .map(|sensor| truncate_message(&sensor.output))
        .collect();
    let digests: Vec<Option<String>> = report
        .sensors
        .iter()
        .map(|sensor| {
            cache_digest(
                root,
                sensor,
                specs.get(sensor.name.as_str()).copied(),
                config_bytes,
                pre_digests,
            )
        })
        .collect();
    let outcomes: Vec<do_harness_db::SensorOutcome<'_>> = report
        .sensors
        .iter()
        .zip(&messages)
        .zip(&digests)
        .filter(|((sensor, _), _)| {
            sensor.execution == crate::report::Execution::Ran && !skipped.contains(&sensor.name)
        })
        .map(|((sensor, message), digest)| do_harness_db::SensorOutcome {
            beat: do_harness_db::NewBeat {
                task_id,
                scope: Some(scope.clone()),
                beat_type: do_harness_types::BeatType::Sensor,
                status: do_harness_types::BeatStatus::for_outcome(sensor.ok, sensor.allow_failure),
                sensor_exit_code: sensor.exit_code,
                sensor_name: Some(&sensor.name),
                started_at: now,
                completed_at: Some(now),
            },
            ok: sensor.ok,
            message: Some(message),
            input_digest: digest.as_deref(),
        })
        .collect();
    do_harness_db::record_verify_batch(&conn, &outcomes).await?;

    for sensor in &report.sensors {
        if sensor.execution != crate::report::Execution::Ran || skipped.contains(&sensor.name) {
            continue;
        }
        if let Some(findings) = sensor.findings {
            let findings = i64::try_from(findings).unwrap_or(i64::MAX);
            do_harness_db::upsert_sensor_findings(&conn, &sensor.name, findings, now).await?;
        }
    }
    Ok(())
}

/// Input identity to store on a beat, or `None` when the sensor is not
/// cache-eligible.
///
/// Eligibility is deliberately narrow: the process ran and exited zero, no
/// `SKIP:`/`COVERAGE:` marker or findings were reported, no artifact globs
/// are declared (reuse would not re-produce outputs), the sensor declares
/// `inputs`, the pre-execution identity is known, and the identity is
/// unchanged after execution (a file that changed while the sensor ran
/// invalidates the entry rather than pinning a stale digest).
fn cache_digest(
    root: &Path,
    sensor: &crate::report::SensorResult,
    spec: Option<&crate::config::SensorSpec>,
    config_bytes: Option<&[u8]>,
    pre_digests: &BTreeMap<String, String>,
) -> Option<String> {
    let spec = spec?;
    if !sensor.ok || sensor.exit_code != Some(0) || sensor.warned {
        return None;
    }
    if !spec.artifacts.is_empty() && crate::artifacts::resolve(root, &spec.artifacts).is_empty() {
        return None;
    }
    let observation_only = spec.inputs.is_empty()
        || sensor.findings.is_some_and(|findings| findings > 0)
        || !spec.artifacts.is_empty()
        || sensor
            .output
            .lines()
            .any(|line| line.trim_start().starts_with("COVERAGE:"));
    let pre = pre_digests.get(&sensor.name)?;
    let post = crate::provenance::sensor_digest(root, spec, config_bytes)?;
    (post == *pre).then(|| {
        if observation_only {
            format!("{}{post}", do_harness_db::OBSERVATION_ONLY_PREFIX)
        } else {
            post
        }
    })
}

/// Bounds a sensor output while preserving its first actionable failure and
/// final summary.
fn truncate_message(output: &str) -> String {
    let count = output.chars().count();
    if count <= MAX_SIGNATURE_MESSAGE {
        return output.to_owned();
    }

    let mut offset = 0;
    let mut first_error = None;
    let failure_start = output
        .split_inclusive('\n')
        .find_map(|line| {
            let start = offset;
            offset += line.len();
            let line = line.trim_start();
            if line.starts_with("FAIL [")
                || line.starts_with("ERROR [")
                // nextest prints its exec-failure header as the bare token
                // between rule characters (` execfail `), and diagnostics use
                // the `execfail: …` form; either anchors. A test *name* such
                // as `handles_execfail_semantics` must not.
                || line
                    .split(|c: char| !(c.is_alphanumeric() || c == '-' || c == '_'))
                    .any(|token| token == "execfail")
            {
                Some(start)
            } else if line.starts_with("error:") {
                first_error.get_or_insert(start);
                None
            } else {
                None
            }
        })
        .or(first_error);
    let context_start = failure_start.unwrap_or(0);
    let context_end = output[context_start..]
        .char_indices()
        .nth(FAILURE_CONTEXT_CHARS)
        .map_or(output.len(), |(index, _)| context_start + index);
    let final_start = output
        .char_indices()
        .rev()
        .nth(FINAL_OUTPUT_CHARS - 1)
        .map_or(0, |(index, _)| index);

    let mut message = String::with_capacity(MAX_SIGNATURE_MESSAGE);
    message.push_str("[output truncated; showing failure context and final output]\n");
    if failure_start.is_some() {
        message.push_str("[first failure context]\n");
    } else {
        message.push_str("[initial output]\n");
    }
    message.push_str(&output[context_start..context_end]);
    message.push_str("\n[final output]\n");
    message.push_str(&output[final_start..]);
    message
}

#[cfg(test)]
mod tests;
