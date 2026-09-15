//! Verify-report, `only` filtering, and fail-fast tests.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::BTreeMap;

use super::super::{VerifyOpts, verify};
use super::config_with;
use crate::config::{Config, HooksConfig};

/// A failing sensor fails the report and records its exit code.
#[test]
fn failing_sensor_fails_report() {
    let dir = tempfile::tempdir().expect("tempdir");
    let cfg = config_with(&[("pass", &["true"]), ("fail", &["false"])]);
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
    assert!(!report.ok);
    assert_eq!(report.failed, vec!["fail".to_owned()]);
    let exit_codes: Vec<Option<i32>> = report.sensors.iter().map(|s| s.exit_code).collect();
    assert_eq!(exit_codes, vec![Some(0), Some(1)]);
    let oks: Vec<bool> = report.sensors.iter().map(|s| s.ok).collect();
    assert_eq!(oks, vec![true, false]);
}

/// The `only` filter restricts execution to the named sensor.
#[test]
fn only_filter_runs_subset() {
    let dir = tempfile::tempdir().expect("tempdir");
    let cfg = config_with(&[("pass", &["true"]), ("fail", &["false"])]);
    let report = verify(
        &cfg,
        dir.path(),
        &VerifyOpts {
            fail_fast: false,
            only: vec!["pass".to_owned()],
            exclude: vec![],
            blocked: vec![],
            ..Default::default()
        },
    )
    .expect("verify");
    assert!(report.ok);
    assert_eq!(report.sensors.len(), 1);
    assert_eq!(report.sensors[0].name, "pass");
}

/// An unknown `only` name is rejected before anything runs.
#[test]
fn unknown_only_name_errors() {
    let dir = tempfile::tempdir().expect("tempdir");
    let cfg = config_with(&[("pass", &["true"])]);
    let err = verify(
        &cfg,
        dir.path(),
        &VerifyOpts {
            fail_fast: false,
            only: vec!["nope".to_owned()],
            exclude: vec![],
            blocked: vec![],
            ..Default::default()
        },
    )
    .expect_err("verify must fail");
    assert!(err.to_string().contains("nope"));
    assert!(err.to_string().contains("pass"));
}

/// With `fail_fast`, execution stops at the first failing sensor.
#[test]
fn fail_fast_stops_at_first_failure() {
    let dir = tempfile::tempdir().expect("tempdir");
    let cfg = config_with(&[("fail", &["false"]), ("true", &["true"])]);
    let report = verify(
        &cfg,
        dir.path(),
        &VerifyOpts {
            fail_fast: true,
            only: vec![],
            exclude: vec![],
            blocked: vec![],
            ..Default::default()
        },
    )
    .expect("verify");
    assert!(!report.ok);
    assert_eq!(report.sensors.len(), 1);
    assert_eq!(report.sensors[0].name, "fail");
    assert_eq!(report.failed, vec!["fail".to_owned()]);
}

/// A generic pack with no sensors verifies trivially.
#[test]
fn verify_with_no_effective_sensors_succeeds() {
    let dir = tempfile::tempdir().expect("tempdir");
    let cfg = Config {
        language: Some("generic".to_owned()),
        hooks: HooksConfig {
            pre_commit: vec![],
            pre_push: vec![],
        },
        sensors: vec![],
        signal_sets: BTreeMap::new(),
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
    assert!(report.sensors.is_empty());
    assert!(report.failed.is_empty());
}
