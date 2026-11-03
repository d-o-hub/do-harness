//! `verify --dry-run` plans the selected sensors without spawning them.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::super::{VerifyOpts, verify};
use super::config_with;
use crate::report::Execution;

/// A dry run reports every selected sensor as planned, with no exit code and
/// no process side effect.
#[test]
fn dry_run_plans_without_spawning() {
    let dir = tempfile::tempdir().expect("tempdir");
    let marker = dir.path().join("ran");
    let script = format!("echo ran > {}", marker.display());
    let cfg = config_with(&[("marker", &["sh", "-c", &script])]);

    let report = verify(
        &cfg,
        dir.path(),
        &VerifyOpts {
            dry_run: true,
            ..Default::default()
        },
    )
    .expect("verify");

    assert!(!report.ok, "a plan is not a pass");
    assert!(report.failed.is_empty(), "nothing failed: it never ran");
    assert_eq!(report.sensors.len(), 1);
    let sensor = &report.sensors[0];
    assert_eq!(sensor.execution, Execution::Planned);
    assert!(!sensor.ok);
    assert_eq!(sensor.exit_code, None);
    assert_eq!(sensor.duration_ms, 0);
    assert!(!marker.exists(), "the sensor process must not be spawned");
}

/// An empty selection is still a plan, not a vacuous pass.
#[test]
fn dry_run_with_no_sensors_is_not_green() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut cfg = config_with(&[]);
    // `generic` has no built-in pack, so the selection is genuinely empty.
    cfg.language = Some("generic".to_owned());

    let report = verify(
        &cfg,
        dir.path(),
        &VerifyOpts {
            dry_run: true,
            ..Default::default()
        },
    )
    .expect("verify");

    assert!(!report.ok);
    assert!(report.sensors.is_empty());
}
