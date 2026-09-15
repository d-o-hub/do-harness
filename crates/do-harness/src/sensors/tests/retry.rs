//! Retry, timeout, and transient-exit-code tests.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use super::super::{VerifyOpts, verify};
use crate::config::{Config, HooksConfig, SensorSpec};

/// A sensor configured to retry retries upon failure until it succeeds.
#[test]
fn retries_failing_sensor_until_success() {
    let dir = tempfile::tempdir().expect("tempdir");
    let counter_file = dir.path().join("counter.txt");
    let script = format!(
        "count=$(cat '{}' 2>/dev/null || echo 0)\ncount=$((count + 1))\necho $count > '{}'\nif [ $count -lt 3 ]; then exit 1; fi",
        counter_file.display(),
        counter_file.display()
    );
    let cfg = Config {
        language: None,
        hooks: HooksConfig::default(),
        signal_sets: BTreeMap::new(),
        sensors: vec![SensorSpec {
            name: "flaky".to_owned(),
            argv: vec!["sh".to_owned(), "-c".to_owned(), script],
            retry: Some(3),
            timeout: None,
            severity: None,
            allow_failure: false,
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
    assert!(report.sensors[0].ok);
    assert_eq!(
        std::fs::read_to_string(&counter_file)
            .expect("read counter")
            .trim(),
        "3"
    );
}

/// A sensor configured with a timeout budget is aborted when it hangs.
#[test]
fn times_out_hanging_sensor() {
    let dir = tempfile::tempdir().expect("tempdir");
    let cfg = Config {
        language: None,
        hooks: HooksConfig::default(),
        signal_sets: BTreeMap::new(),
        sensors: vec![SensorSpec {
            name: "hang".to_owned(),
            argv: vec!["sleep".to_owned(), "10".to_owned()],
            retry: None,
            timeout: Some(1),
            severity: None,
            allow_failure: false,
            transient_exit_codes: vec![],
            artifacts: Vec::new(),
            coverage_inputs: Vec::new(),
            when_changed: vec![],
        }],
        jobs: None,
    };

    let start = Instant::now();
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

    let duration = start.elapsed();
    assert!(duration < Duration::from_secs(5));
    assert!(!report.ok);
    assert_eq!(report.failed, vec!["hang".to_owned()]);
    assert!(!report.sensors[0].ok);
    assert!(report.sensors[0].output.contains("timed out after 1s"));
}

/// Restricting retries to `transient_exit_codes` does not retry non-transient exit codes.
#[test]
fn transient_exit_codes_restricts_retries() {
    let dir = tempfile::tempdir().expect("tempdir");
    let counter_file = dir.path().join("transient_counter.txt");
    let script = format!(
        "count=$(cat '{}' 2>/dev/null || echo 0)\ncount=$((count + 1))\necho $count > '{}'\nexit 1",
        counter_file.display(),
        counter_file.display()
    );
    let cfg = Config {
        language: None,
        hooks: HooksConfig::default(),
        signal_sets: BTreeMap::new(),
        sensors: vec![SensorSpec {
            name: "transient_check".to_owned(),
            argv: vec!["sh".to_owned(), "-c".to_owned(), script],
            retry: Some(3),
            timeout: None,
            severity: None,
            allow_failure: false,
            transient_exit_codes: vec![75],
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

    assert!(!report.ok);
    assert_eq!(report.failed, vec!["transient_check".to_owned()]);
    assert_eq!(
        std::fs::read_to_string(&counter_file)
            .expect("read counter")
            .trim(),
        "1"
    );
}
