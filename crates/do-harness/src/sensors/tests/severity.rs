//! Severity, strict-promotion, quarantine, and findings-ratchet tests.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::BTreeMap;

use super::*;
use crate::baselines::Baselines;
use crate::config::SensorSeverity;

/// Builds a config with one sensor of the given severity and a `feedback`
/// signal set naming it.
fn severity_config(name: &str, argv: &[&str], severity: SensorSeverity) -> Config {
    let mut signal_sets = BTreeMap::new();
    signal_sets.insert("feedback".to_owned(), vec![name.to_owned()]);
    Config {
        language: None,
        hooks: HooksConfig::default(),
        signal_sets,
        sensors: vec![SensorSpec {
            name: name.to_owned(),
            argv: argv.iter().map(|a| (*a).to_owned()).collect(),
            retry: None,
            timeout: None,
            severity: Some(severity),
            allow_failure: false,
            transient_exit_codes: vec![],
            artifacts: Vec::new(),
            coverage_inputs: Vec::new(),
            when_changed: vec![],
        }],
        jobs: None,
    }
}

/// Builds a one-entry ratchet baseline set.
fn baselines_for(name: &str, max: u64) -> Baselines {
    let mut baselines = Baselines::default();
    baselines.bless(name, max);
    baselines
}

/// A warn-severity failure is advisory in a non-strict run but fails under
/// `--strict`.
#[test]
fn warn_severity_is_advisory_but_strict_fails() {
    let dir = tempfile::tempdir().expect("tempdir");
    let cfg = severity_config("advisory", &["false"], SensorSeverity::Warn);

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
    assert!(report.ok, "non-strict warn failure must not fail the gate");
    assert!(report.failed.is_empty());
    assert!(!report.sensors[0].ok);
    assert!(report.sensors[0].allow_failure);

    let strict = verify(
        &cfg,
        dir.path(),
        &VerifyOpts {
            strict: true,
            ..Default::default()
        },
    )
    .expect("verify");
    assert!(!strict.ok, "--strict must promote warn failures");
    assert_eq!(strict.failed, vec!["advisory".to_owned()]);
    assert!(!strict.sensors[0].allow_failure);
}

/// `--strict` never promotes advisory failures inside the `feedback` set.
#[test]
fn strict_promotion_skips_feedback_set() {
    let dir = tempfile::tempdir().expect("tempdir");
    let cfg = severity_config("advisory", &["false"], SensorSeverity::Warn);

    let report = verify(
        &cfg,
        dir.path(),
        &VerifyOpts {
            strict: true,
            set: Some("feedback".to_owned()),
            ..Default::default()
        },
    )
    .expect("verify");
    assert!(report.ok, "feedback strict must stay advisory");
    assert!(report.failed.is_empty());
    assert!(report.sensors[0].allow_failure);
}

/// A quarantined warn sensor is skipped with an advisory verdict and never
/// cancels siblings under `--fail-fast`.
#[test]
fn quarantined_sensor_is_skipped_advisory() {
    let dir = tempfile::tempdir().expect("tempdir");
    let cfg = config_with(&[
        ("noisy", &["sh", "-c", "touch marker"]),
        ("after", &["true"]),
    ]);
    let report = verify(
        &cfg,
        dir.path(),
        &VerifyOpts {
            fail_fast: true,
            quarantined: vec!["noisy".to_owned()],
            ..Default::default()
        },
    )
    .expect("verify");
    assert!(report.ok);
    assert!(report.failed.is_empty());
    assert_eq!(
        report.sensors.len(),
        2,
        "quarantine must not cancel siblings"
    );
    assert!(!report.sensors[0].ok);
    assert!(report.sensors[0].allow_failure);
    assert!(report.sensors[0].warned);
    assert!(report.sensors[0].output.contains("quarantined"));
    assert!(!dir.path().join("marker").exists());
    assert!(report.sensors[1].ok);
}

/// A findings count above the blessed baseline is a hard regression, even for
/// a warn-severity sensor.
#[test]
fn ratchet_regression_fails_even_warn_severity() {
    let dir = tempfile::tempdir().expect("tempdir");
    let cfg = severity_config(
        "noisy",
        &["sh", "-c", "echo 'FINDINGS: 5'; exit 0"],
        SensorSeverity::Warn,
    );
    let report = verify(
        &cfg,
        dir.path(),
        &VerifyOpts {
            baselines: baselines_for("noisy", 2),
            ..Default::default()
        },
    )
    .expect("verify");
    assert!(!report.ok, "a ratchet regression must fail the gate");
    assert_eq!(report.failed, vec!["noisy".to_owned()]);
    assert!(!report.sensors[0].allow_failure);
    assert_eq!(report.sensors[0].findings, Some(5));
    assert_eq!(report.sensors[0].baseline, Some(2));
    assert!(report.sensors[0].output.contains("ratchet regression"));
}

/// Findings at or below the baseline warn with the recorded counts, and the
/// last `FINDINGS:` marker wins.
#[test]
fn findings_below_baseline_warn_with_counts() {
    let dir = tempfile::tempdir().expect("tempdir");
    let cfg = severity_config(
        "noisy",
        &["sh", "-c", "echo 'FINDINGS: 1'; echo 'FINDINGS: 2'; exit 1"],
        SensorSeverity::Error,
    );
    let report = verify(
        &cfg,
        dir.path(),
        &VerifyOpts {
            baselines: baselines_for("noisy", 4),
            ..Default::default()
        },
    )
    .expect("verify");
    assert!(report.ok, "below-baseline findings must stay advisory");
    assert!(report.failed.is_empty());
    assert!(!report.sensors[0].ok);
    assert!(report.sensors[0].allow_failure);
    assert!(report.sensors[0].warned);
    assert_eq!(report.sensors[0].findings, Some(2), "last marker wins");
    assert_eq!(report.sensors[0].baseline, Some(4));
}

/// Zero findings leave a passing sensor passing and never warn.
#[test]
fn zero_findings_pass_without_warning() {
    let dir = tempfile::tempdir().expect("tempdir");
    let cfg = severity_config(
        "clean",
        &["sh", "-c", "echo 'FINDINGS: 0'; exit 0"],
        SensorSeverity::Error,
    );
    let report = verify(
        &cfg,
        dir.path(),
        &VerifyOpts {
            baselines: baselines_for("clean", 0),
            ..Default::default()
        },
    )
    .expect("verify");
    assert!(report.ok);
    assert!(report.sensors[0].ok);
    assert!(!report.sensors[0].warned);
    assert_eq!(report.sensors[0].findings, Some(0));
}

/// Without a blessed baseline the exit code and severity decide.
#[test]
fn findings_without_baseline_keep_exit_semantics() {
    let dir = tempfile::tempdir().expect("tempdir");
    let cfg = severity_config(
        "unbaselined",
        &["sh", "-c", "echo 'FINDINGS: 9'; exit 0"],
        SensorSeverity::Error,
    );
    let report = verify(&cfg, dir.path(), &VerifyOpts::default()).expect("verify");
    assert!(report.ok);
    assert_eq!(report.sensors[0].findings, Some(9));
    assert_eq!(report.sensors[0].baseline, None);
}
