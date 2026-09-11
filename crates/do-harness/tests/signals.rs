//! Acceptance fixtures for development signal sets (`verify --set`).
//!
//! These tests drive the real `do-harness` binary against fixture configs
//! with instant `true`/`false` sensors, so set selection is proven
//! computationally rather than assumed.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::Path;
use std::process::Command;

use serde_json::Value;

/// Builds a `do-harness --root <root>` command using the real binary.
fn harness(root: &Path) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_do-harness"));
    cmd.arg("--root").arg(root);
    cmd
}

/// Runs a harness command, returning (exit code, `stdout`, `stderr`).
fn run(cmd: &mut Command) -> (Option<i32>, String, String) {
    let output = cmd.output().expect("spawn do-harness");
    (
        output.status.code(),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

/// Writes a fixture config with instant sensors and two signal sets.
fn write_sets_config(dir: &Path) {
    let text = r#"
language = "rust"

[signal-sets]
feedback = ["fast-pass"]
verification = ["fast-pass", "fast-fail"]

[[sensors]]
name = "fast-pass"
argv = ["true"]

[[sensors]]
name = "fast-fail"
argv = ["false"]
"#;
    std::fs::write(dir.join("do-harness.toml"), text).unwrap();
}

/// `verify --set feedback` runs only that set and reports it in JSON.
#[test]
fn verify_set_feedback_runs_only_that_set() {
    let dir = tempfile::tempdir().unwrap();
    write_sets_config(dir.path());

    let (code, stdout, _stderr) = run(harness(dir.path())
        .arg("verify")
        .arg("--set")
        .arg("feedback")
        .arg("--format")
        .arg("json"));
    assert_eq!(code, Some(0), "feedback run must exit 0:\n{stdout}");
    let report: Value = serde_json::from_str(&stdout).expect("json report");
    assert_eq!(report["signal_set"], serde_json::json!("feedback"));
    assert_eq!(report["ok"], serde_json::json!(true));
    let sensors: Vec<&str> = report["sensors"]
        .as_array()
        .expect("sensors array")
        .iter()
        .map(|s| s["name"].as_str().expect("sensor name"))
        .collect();
    assert_eq!(sensors, vec!["fast-pass"]);
}

/// `verify --set verification` runs its own wider set, including the failure.
#[test]
fn verify_set_verification_runs_wider_set_with_failure() {
    let dir = tempfile::tempdir().unwrap();
    write_sets_config(dir.path());

    let (code, stdout, _stderr) = run(harness(dir.path())
        .arg("verify")
        .arg("--set")
        .arg("verification")
        .arg("--format")
        .arg("json"));
    assert_eq!(code, Some(1), "failing set must exit 1:\n{stdout}");
    let report: Value = serde_json::from_str(&stdout).expect("json report");
    assert_eq!(report["signal_set"], serde_json::json!("verification"));
    assert_eq!(report["ok"], serde_json::json!(false));
    let failed = report["failed"].as_array().expect("failed array");
    assert_eq!(failed, &vec![serde_json::json!("fast-fail")]);
}

/// An unknown signal set fails loudly with exit code 2.
#[test]
fn verify_unknown_set_fails_loudly() {
    let dir = tempfile::tempdir().unwrap();
    write_sets_config(dir.path());

    let (code, _stdout, stderr) = run(harness(dir.path())
        .arg("verify")
        .arg("--set")
        .arg("release")
        .arg("--format")
        .arg("json"));
    assert_eq!(code, Some(2), "unknown set must be a usage error");
    assert!(
        stderr.contains("release") && stderr.contains("feedback"),
        "error must name the unknown set and the available ones: {stderr}"
    );
}

/// `--only` narrows within the selected set; a sensor outside the set is rejected.
#[test]
fn verify_only_outside_set_is_rejected() {
    let dir = tempfile::tempdir().unwrap();
    write_sets_config(dir.path());

    let (code, _stdout, stderr) = run(harness(dir.path())
        .arg("verify")
        .arg("--set")
        .arg("feedback")
        .arg("--only")
        .arg("fast-fail")
        .arg("--format")
        .arg("json"));
    assert_eq!(code, Some(2), "--only outside the set must fail");
    assert!(
        stderr.contains("fast-fail"),
        "error must name the offending sensor: {stderr}"
    );
}

/// A signal set referencing an unknown sensor fails config validation.
#[test]
fn unknown_sensor_in_set_fails_config_validation() {
    let dir = tempfile::tempdir().unwrap();
    let text = r#"
[signal-sets]
feedback = ["ghost"]

[[sensors]]
name = "real"
argv = ["true"]
"#;
    std::fs::write(dir.path().join("do-harness.toml"), text).unwrap();

    let (code, _stdout, stderr) = run(harness(dir.path())
        .arg("verify")
        .arg("--set")
        .arg("feedback"));
    assert_eq!(
        code,
        Some(2),
        "dangling set reference must be a usage error"
    );
    assert!(
        stderr.contains("ghost"),
        "error must name the unknown sensor: {stderr}"
    );
}

/// Duplicate names inside a signal set are rejected deterministically.
#[test]
fn duplicate_names_in_set_are_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let text = r#"
[signal-sets]
feedback = ["real", "real"]

[[sensors]]
name = "real"
argv = ["true"]
"#;
    std::fs::write(dir.path().join("do-harness.toml"), text).unwrap();

    let (code, _stdout, stderr) = run(harness(dir.path())
        .arg("verify")
        .arg("--set")
        .arg("feedback"));
    assert_eq!(code, Some(2), "duplicate set entries must be a usage error");
    assert!(
        stderr.contains("duplicate"),
        "error must say duplicate: {stderr}"
    );
}

/// Legacy configs without `[signal-sets]`: verification/release run the full
/// list, feedback fails loudly, and plain `verify` is untouched.
#[test]
fn legacy_config_without_sets_keeps_behavior() {
    let dir = tempfile::tempdir().unwrap();
    let text = r#"
[[sensors]]
name = "a"
argv = ["true"]

[[sensors]]
name = "b"
argv = ["true"]
"#;
    std::fs::write(dir.path().join("do-harness.toml"), text).unwrap();

    let (code, stdout, _) = run(harness(dir.path())
        .arg("verify")
        .arg("--set")
        .arg("verification")
        .arg("--format")
        .arg("json"));
    assert_eq!(code, Some(0), "verification falls back to the full list");
    let report: Value = serde_json::from_str(&stdout).expect("json report");
    assert_eq!(report["signal_set"], serde_json::json!("verification"));

    let (code, _, stderr) = run(harness(dir.path())
        .arg("verify")
        .arg("--set")
        .arg("feedback"));
    assert_eq!(code, Some(2), "feedback has no implicit guarantee");
    assert!(
        stderr.contains("feedback"),
        "error must name the set: {stderr}"
    );

    let (code, stdout, _) = run(harness(dir.path())
        .arg("verify")
        .arg("--format")
        .arg("json"));
    assert_eq!(code, Some(0), "plain verify is unchanged");
    let report: Value = serde_json::from_str(&stdout).expect("json report");
    assert!(
        report.get("signal_set").is_none(),
        "no-set runs omit signal_set"
    );
}

/// `list --sets` exposes the configured set names for runtimes.
#[test]
fn list_sets_exposes_configured_names() {
    let dir = tempfile::tempdir().unwrap();
    write_sets_config(dir.path());

    let (code, stdout, _) = run(harness(dir.path())
        .arg("list")
        .arg("--sets")
        .arg("--format")
        .arg("json"));
    assert_eq!(code, Some(0), "list --sets must exit 0:\n{stdout}");
    let names: Value = serde_json::from_str(&stdout).expect("json names");
    assert_eq!(
        names,
        serde_json::json!(["feedback", "verification"]),
        "set names in config order"
    );
}
