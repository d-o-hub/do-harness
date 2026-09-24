//! Optional persistence of verify runs into the agent-state database.

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
pub async fn struck_sensors(
    root: &Path,
    names: &[String],
    task_id: Option<i64>,
) -> Result<Vec<String>> {
    let conn = do_harness_db::connect_and_migrate(root).await?;
    let mut struck = Vec::new();
    for name in names {
        let sig =
            do_harness_db::get_error_signature(&conn, &format!("sensor:{name}"), task_id).await?;
        if sig.is_some_and(|s| s.attempt_count >= FAIL_FAST_STRIKES) {
            struck.push(name.clone());
        }
    }
    Ok(struck)
}

/// Records each sensor result atomically, scoped to `task_id`: the beat and
/// its error-signature update (bump on failure, reset on pass) commit in one
/// transaction, and any observed findings count is upserted. Skipped sensors
/// (halted or quarantined) are omitted.
///
/// # Errors
///
/// Returns an error when the state database cannot be initialized or written.
pub async fn record_verify(
    root: &Path,
    report: &VerifyReport,
    skipped: &[String],
    task_id: Option<i64>,
) -> Result<()> {
    let conn = do_harness_db::connect_and_migrate(root).await?;
    let now = do_harness_db::unix_now();
    let messages: Vec<String> = report
        .sensors
        .iter()
        .map(|sensor| truncate_message(&sensor.output))
        .collect();
    let outcomes: Vec<do_harness_db::SensorOutcome<'_>> = report
        .sensors
        .iter()
        .zip(&messages)
        .filter(|(sensor, _)| !skipped.contains(&sensor.name))
        .map(|(sensor, message)| do_harness_db::SensorOutcome {
            beat: do_harness_db::NewBeat {
                task_id,
                beat_type: "sensor",
                status: if sensor.ok {
                    "ok"
                } else if sensor.allow_failure {
                    "warn"
                } else {
                    "failed"
                },
                sensor_exit_code: sensor.exit_code,
                sensor_name: Some(&sensor.name),
                started_at: now,
                completed_at: Some(now),
            },
            ok: sensor.ok,
            message: Some(message),
        })
        .collect();
    do_harness_db::record_verify_batch(&conn, &outcomes).await?;

    for sensor in &report.sensors {
        if skipped.contains(&sensor.name) {
            continue;
        }
        if let Some(findings) = sensor.findings {
            let findings = i64::try_from(findings).unwrap_or(i64::MAX);
            do_harness_db::upsert_sensor_findings(&conn, &sensor.name, findings, now).await?;
        }
    }
    Ok(())
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
                || line.contains("execfail")
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
