//! Advisory (`allow_failure`, `SKIP:` marker) behavior tests.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::BTreeMap;

use super::super::{VerifyOpts, verify};
use super::config_with;
use crate::config::{Config, HooksConfig, SensorSpec};

/// An `allow_failure` sensor never flips the gate to fail, but failure is recorded.
#[test]
fn allow_failure_sensor_does_not_fail_gate_but_surfaces_output() {
    let dir = tempfile::tempdir().expect("tempdir");
    let cfg = Config {
        language: None,
        hooks: HooksConfig::default(),
        signal_sets: BTreeMap::new(),
        sensors: vec![SensorSpec {
            name: "advisory".to_owned(),
            argv: vec![
                "sh".to_owned(),
                "-c".to_owned(),
                "echo 'something wrong'; exit 1".to_owned(),
            ],
            retry: None,
            timeout: None,
            severity: None,
            allow_failure: true,
            transient_exit_codes: vec![],
            artifacts: Vec::new(),
            coverage_inputs: Vec::new(),
            when_changed: vec![],
        }],
        jobs: None,
    };

    let report = verify(
        &cfg,
        dir.path(),
        &VerifyOpts {
            fail_fast: false,
            only: vec![],
            exclude: vec![],
            blocked: vec![],
            ..Default::default()
        },
    )
    .expect("verify");

    assert!(report.ok);
    assert!(report.failed.is_empty());
    assert!(!report.sensors[0].ok);
    assert!(report.sensors[0].allow_failure);
    assert!(report.sensors[0].output.contains("something wrong"));
}

/// A passing sensor whose output carries a `SKIP:` marker is warned, not
/// failed: the local gate stays green while evidence records the skip.
#[test]
fn skip_marker_warns_without_failing_gate() {
    let dir = tempfile::tempdir().expect("tempdir");
    let cfg = config_with(&[(
        "skippy",
        &["bash", "-c", "echo 'SKIP: tool missing'; exit 0"],
    )]);
    let report = verify(
        &cfg,
        dir.path(),
        &VerifyOpts {
            fail_fast: false,
            only: vec![],
            exclude: vec![],
            blocked: vec![],
            ..Default::default()
        },
    )
    .expect("verify");

    assert!(report.ok);
    assert!(report.failed.is_empty());
    assert!(report.sensors[0].ok);
    assert!(report.sensors[0].warned);
}
