//! Optional persistence of verify runs into the agent-state database.

use std::path::Path;

use anyhow::Result;

use crate::report::VerifyReport;

/// Maximum characters of a failing sensor's output stored in an error
/// signature message.
const MAX_SIGNATURE_MESSAGE: usize = 500;

/// Consecutive failures after which `verify --record` halts a sensor.
pub const FAIL_FAST_STRIKES: i64 = 3;

/// Returns the sensor names whose `sensor:<name>` error signature for `task_id`
/// has `attempt_count >= FAIL_FAST_STRIKES`; these are halted by verify.
///
/// # Errors
///
/// Returns an error when the state database cannot be initialized or queried.
pub async fn blocked_sensors(
    root: &Path,
    names: &[String],
    task_id: Option<i64>,
) -> Result<Vec<String>> {
    let conn = do_harness_db::connect_and_migrate(root).await?;
    let mut blocked = Vec::new();
    for name in names {
        let sig =
            do_harness_db::get_error_signature(&conn, &format!("sensor:{name}"), task_id).await?;
        if sig.is_some_and(|s| s.attempt_count >= FAIL_FAST_STRIKES) {
            blocked.push(name.clone());
        }
    }
    Ok(blocked)
}

/// Records each sensor result atomically, scoped to `task_id`: the beat and
/// its error-signature update (bump on failure, reset on pass) commit in one
/// transaction. Halted sensors are skipped.
///
/// # Errors
///
/// Returns an error when the state database cannot be initialized or written.
pub async fn record_verify(
    root: &Path,
    report: &VerifyReport,
    blocked: &[String],
    task_id: Option<i64>,
) -> Result<()> {
    let conn = do_harness_db::connect_and_migrate(root).await?;
    let now = do_harness_db::unix_now();
    for sensor in &report.sensors {
        if blocked.contains(&sensor.name) {
            continue;
        }
        do_harness_db::record_sensor_outcome(
            &conn,
            &do_harness_db::NewBeat {
                task_id,
                beat_type: "sensor",
                status: if sensor.ok { "ok" } else { "failed" },
                sensor_exit_code: sensor.exit_code,
                sensor_name: Some(&sensor.name),
                started_at: now,
                completed_at: Some(now),
            },
            sensor.ok,
            Some(&truncate_message(&sensor.output)),
        )
        .await?;
    }
    Ok(())
}

/// Bounds a sensor output to the last [`MAX_SIGNATURE_MESSAGE`] characters.
fn truncate_message(output: &str) -> String {
    let count = output.chars().count();
    if count <= MAX_SIGNATURE_MESSAGE {
        return output.to_owned();
    }
    output.chars().skip(count - MAX_SIGNATURE_MESSAGE).collect()
}

#[cfg(test)]
mod tests;
