//! Optional persistence of verify runs into the agent-state database.

use std::path::Path;

use anyhow::Result;

use crate::report::VerifyReport;

/// Maximum characters of a failing sensor's output stored in an error
/// signature message.
const MAX_SIGNATURE_MESSAGE: usize = 500;

/// Consecutive failures after which `verify --record` halts a sensor.
pub const FAIL_FAST_STRIKES: i64 = 3;

/// Returns the sensor names whose `sensor:<name>` error signature has
/// `attempt_count >= FAIL_FAST_STRIKES`; these are halted by verify.
///
/// # Errors
///
/// Returns an error when the state database cannot be initialized or queried.
pub async fn blocked_sensors(root: &Path, names: &[String]) -> Result<Vec<String>> {
    let conn = do_harness_db::connect_and_migrate(root).await?;
    let mut blocked = Vec::new();
    for name in names {
        let sig = do_harness_db::get_error_signature(&conn, &format!("sensor:{name}")).await?;
        if sig.is_some_and(|s| s.attempt_count >= FAIL_FAST_STRIKES) {
            blocked.push(name.clone());
        }
    }
    Ok(blocked)
}

/// Records each sensor result as a beat and bumps an error signature for
/// every failing sensor that is not halted by the fail-fast policy.
///
/// # Errors
///
/// Returns an error when the state database cannot be initialized or written.
pub async fn record_verify(root: &Path, report: &VerifyReport, blocked: &[String]) -> Result<()> {
    let conn = do_harness_db::connect_and_migrate(root).await?;
    let now = do_harness_db::unix_now();
    for sensor in &report.sensors {
        if blocked.contains(&sensor.name) {
            continue;
        }
        do_harness_db::insert_beat(
            &conn,
            &do_harness_db::NewBeat {
                task_id: None,
                beat_type: "sensor",
                status: if sensor.ok { "ok" } else { "failed" },
                sensor_exit_code: sensor.exit_code,
                started_at: now,
                completed_at: Some(now),
            },
        )
        .await?;
        if !sensor.ok {
            do_harness_db::bump_error_signature(
                &conn,
                &format!("sensor:{}", sensor.name),
                None,
                Some(&truncate_message(&sensor.output)),
            )
            .await?;
        }
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
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use crate::report::SensorResult;

    #[tokio::test(flavor = "current_thread")]
    async fn record_verify_persists_beats_and_signatures() {
        let dir = tempfile::tempdir().unwrap();
        let report = VerifyReport {
            ok: false,
            root: dir.path().display().to_string(),
            failed: vec!["fail".to_owned()],
            sensors: vec![SensorResult {
                name: "fail".to_owned(),
                ok: false,
                exit_code: Some(1),
                duration_ms: 1,
                output: "boom".to_owned(),
            }],
        };

        record_verify(dir.path(), &report, &[]).await.unwrap();

        let conn = do_harness_db::connect_and_migrate(dir.path())
            .await
            .unwrap();
        let beats = do_harness_db::list_beats(&conn, None).await.unwrap();
        assert_eq!(beats.len(), 1);
        assert_eq!(beats[0].beat_type, "sensor");
        assert_eq!(beats[0].status, "failed");
        assert_eq!(beats[0].sensor_exit_code, Some(1));
        let sig = do_harness_db::get_error_signature(&conn, "sensor:fail")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(sig.attempt_count, 1);
        assert_eq!(sig.message.as_deref(), Some("boom"));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn record_verify_skips_signatures_when_all_pass() {
        let dir = tempfile::tempdir().unwrap();
        let report = VerifyReport {
            ok: true,
            root: dir.path().display().to_string(),
            failed: vec![],
            sensors: vec![SensorResult {
                name: "fmt".to_owned(),
                ok: true,
                exit_code: Some(0),
                duration_ms: 1,
                output: String::new(),
            }],
        };

        record_verify(dir.path(), &report, &[]).await.unwrap();

        let conn = do_harness_db::connect_and_migrate(dir.path())
            .await
            .unwrap();
        let beats = do_harness_db::list_beats(&conn, None).await.unwrap();
        assert_eq!(beats.len(), 1);
        assert_eq!(beats[0].status, "ok");
        assert!(
            do_harness_db::get_error_signature(&conn, "sensor:fmt")
                .await
                .unwrap()
                .is_none()
        );
    }

    /// A blocked sensor is skipped entirely: no beat, no signature bump.
    #[tokio::test(flavor = "current_thread")]
    async fn record_verify_skips_blocked_sensor_signature() {
        let dir = tempfile::tempdir().unwrap();
        let report = VerifyReport {
            ok: false,
            root: dir.path().display().to_string(),
            failed: vec!["halted".to_owned()],
            sensors: vec![SensorResult {
                name: "halted".to_owned(),
                ok: false,
                exit_code: None,
                duration_ms: 0,
                output: "halted: ...".to_owned(),
            }],
        };

        record_verify(dir.path(), &report, &["halted".to_owned()])
            .await
            .unwrap();

        let conn = do_harness_db::connect_and_migrate(dir.path())
            .await
            .unwrap();
        let beats = do_harness_db::list_beats(&conn, None).await.unwrap();
        assert!(beats.is_empty());
        assert!(
            do_harness_db::get_error_signature(&conn, "sensor:halted")
                .await
                .unwrap()
                .is_none()
        );
    }

    /// `blocked_sensors` halts names at the strike threshold and ignores the rest.
    #[tokio::test(flavor = "current_thread")]
    async fn blocked_sensors_halts_only_struck_out_names() {
        let dir = tempfile::tempdir().unwrap();
        let conn = do_harness_db::connect_and_migrate(dir.path())
            .await
            .unwrap();
        for _ in 0..FAIL_FAST_STRIKES {
            do_harness_db::bump_error_signature(&conn, "sensor:struck", None, Some("boom"))
                .await
                .unwrap();
        }
        for _ in 0..FAIL_FAST_STRIKES - 1 {
            do_harness_db::bump_error_signature(&conn, "sensor:close", None, Some("boom"))
                .await
                .unwrap();
        }

        let names = vec!["struck".to_owned(), "close".to_owned(), "fresh".to_owned()];
        let blocked = blocked_sensors(dir.path(), &names).await.unwrap();
        assert_eq!(blocked, vec!["struck".to_owned()]);
        assert!(blocked_sensors(dir.path(), &[]).await.unwrap().is_empty());
    }

    /// Long outputs are bounded so signatures stay readable.
    #[test]
    fn truncate_message_bounds_to_last_chars() {
        let long = "x".repeat(1200);
        let truncated = truncate_message(&long);
        assert_eq!(truncated.chars().count(), MAX_SIGNATURE_MESSAGE);
        assert!(truncated.ends_with("xxx"));
        assert_eq!(truncate_message("short"), "short");
    }
}
