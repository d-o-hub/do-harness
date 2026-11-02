//! A `test` sensor that ran zero tests must fail, not pass (AGENTS.md §4).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::super::{VerifyOpts, verify};
use super::config_with;

/// A delegated `test` sensor inherits its runner's zero-test behaviour. Node's
/// built-in runner exits 0 with an empty suite, so the runner reads the total
/// summary and fails the gate instead of reporting a vacuous pass.
#[tokio::test(flavor = "current_thread")]
async fn a_test_sensor_that_ran_zero_tests_fails() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = config_with(&[(
        "test",
        &[
            "sh",
            "-c",
            "printf '\\342\\204\\271 tests 0\\n\\342\\204\\271 pass 0\\n'",
        ],
    )]);
    let report = verify(&cfg, dir.path(), &VerifyOpts::default()).unwrap();
    assert!(!report.ok, "a zero-test run must not pass");
    assert_eq!(report.failed, vec!["test".to_owned()]);
    let sensor = &report.sensors[0];
    assert!(
        sensor.output.contains("the test sensor ran zero tests"),
        "the failure must explain itself: {}",
        sensor.output
    );
}

/// The same output from a sensor that is not the test gate is not vacuous: only
/// the `test` sensor carries the zero-test contract.
#[tokio::test(flavor = "current_thread")]
async fn a_non_test_sensor_reporting_zero_tests_still_passes() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = config_with(&[("lint", &["sh", "-c", "printf '\\342\\204\\271 tests 0\\n'"])]);
    let report = verify(&cfg, dir.path(), &VerifyOpts::default()).unwrap();
    assert!(
        report.ok,
        "only the test sensor carries the zero-test contract"
    );
}

/// A populated suite passes unchanged.
#[tokio::test(flavor = "current_thread")]
async fn a_populated_test_sensor_passes() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = config_with(&[(
        "test",
        &[
            "sh",
            "-c",
            "printf '\\342\\204\\271 tests 2\\n\\342\\204\\271 pass 2\\n'",
        ],
    )]);
    let report = verify(&cfg, dir.path(), &VerifyOpts::default()).unwrap();
    assert!(report.ok, "a populated suite must pass");
}
