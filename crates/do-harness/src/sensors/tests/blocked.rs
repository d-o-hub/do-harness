//! Blocked-sensor (halted, unexecuted) behavior tests.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::super::{VerifyOpts, verify};
use super::config_with;

/// A blocked sensor is not executed: no marker file appears.
#[test]
fn blocked_sensor_is_not_executed() {
    let dir = tempfile::tempdir().expect("tempdir");
    let cfg = config_with(&[("mark", &["sh", "-c", "touch marker"])]);
    let report = verify(
        &cfg,
        dir.path(),
        &VerifyOpts {
            fail_fast: false,
            only: vec![],
            exclude: vec![],
            blocked: vec!["mark".to_owned()],
            ..Default::default()
        },
    )
    .expect("verify");
    assert!(!report.sensors[0].ok);
    assert_eq!(report.sensors[0].exit_code, None);
    assert_eq!(report.sensors[0].duration_ms, 0);
    assert!(report.sensors[0].output.contains("halted"));
    assert!(!dir.path().join("marker").exists());
}

/// A blocked sensor fails the report while unblocked sensors still run.
#[test]
fn blocked_sensor_counts_as_failed() {
    let dir = tempfile::tempdir().expect("tempdir");
    let cfg = config_with(&[("ok", &["true"]), ("halt", &["true"])]);
    let report = verify(
        &cfg,
        dir.path(),
        &VerifyOpts {
            fail_fast: false,
            only: vec![],
            exclude: vec![],
            blocked: vec!["halt".to_owned()],
            ..Default::default()
        },
    )
    .expect("verify");
    assert!(!report.ok);
    assert_eq!(report.failed, vec!["halt".to_owned()]);
    assert!(report.sensors[0].ok);
    assert_eq!(report.sensors[0].name, "ok");
    assert!(!report.sensors[1].ok);
}

/// With `fail_fast`, a blocked sensor halts the run like any failure.
#[test]
fn fail_fast_stops_at_first_blocked_sensor() {
    let dir = tempfile::tempdir().expect("tempdir");
    let cfg = config_with(&[("halt", &["true"]), ("after", &["true"])]);
    let report = verify(
        &cfg,
        dir.path(),
        &VerifyOpts {
            fail_fast: true,
            only: vec![],
            exclude: vec![],
            blocked: vec!["halt".to_owned()],
            ..Default::default()
        },
    )
    .expect("verify");
    assert!(!report.ok);
    assert_eq!(report.sensors.len(), 1);
    assert_eq!(report.sensors[0].name, "halt");
    assert!(!report.sensors[0].ok);
    assert_eq!(report.failed, vec!["halt".to_owned()]);
}
