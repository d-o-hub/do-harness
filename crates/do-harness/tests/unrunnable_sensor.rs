//! A sensor that never started must not look like a gate that ran and failed.
//!
//! `verify` exits 2 when every selected sensor was unrunnable, and the JSON
//! report lists them in a top-level `unrunnable` array beside `failed`.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::{Path, PathBuf};

use serde_json::Value;

mod support;

use support::{harness, run};

/// A workspace with a passing, a failing, and an unspawnable sensor.
fn fixture() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_path_buf();
    std::fs::write(
        root.join("do-harness.toml"),
        r#"
[[sensors]]
name = "green"
argv = ["bash", "-c", "echo green-sensor-ran; exit 0"]

[[sensors]]
name = "red"
argv = ["bash", "-c", "echo red-sensor-ran; exit 3"]

[[sensors]]
name = "ghost"
argv = ["definitely-not-an-installed-binary-xyz"]
"#,
    )
    .unwrap();
    (dir, root)
}

fn verify(root: &Path, args: &[&str]) -> (Option<i32>, Value, String) {
    let mut command = harness(root);
    command.args(["verify", "--format", "json"]).args(args);
    let (code, stdout, stderr) = run(&mut command);
    let report: Value = serde_json::from_str(&stdout).expect("verify JSON");
    (code, report, stderr)
}

#[test]
fn every_selected_sensor_unrunnable_exits_2() {
    let (_dir, root) = fixture();
    let (code, report, stderr) = verify(&root, &["--only", "ghost"]);

    assert_eq!(code, Some(2), "stderr={stderr}");
    assert_eq!(report["ok"], false);
    assert_eq!(report["failed"], serde_json::json!(["ghost"]));
    assert_eq!(report["unrunnable"], serde_json::json!(["ghost"]));
    assert_eq!(report["sensors"][0]["exit_code"], Value::Null);
    assert_eq!(report["sensors"][0]["details"]["reason"], "spawn");
}

#[test]
fn strict_unrunnable_sensor_exits_2() {
    let (_dir, root) = fixture();
    let (code, report, stderr) = verify(&root, &["--only", "ghost", "--strict"]);

    assert_eq!(code, Some(2), "stderr={stderr}");
    assert_eq!(report["unrunnable"], serde_json::json!(["ghost"]));
}

#[test]
fn mixed_run_stays_1_and_lists_the_unrunnable_sensor() {
    let (_dir, root) = fixture();
    let (code, report, stderr) = verify(&root, &["--only", "green", "--only", "ghost"]);

    assert_eq!(code, Some(1), "stderr={stderr}");
    assert_eq!(report["failed"], serde_json::json!(["ghost"]));
    assert_eq!(report["unrunnable"], serde_json::json!(["ghost"]));
}

#[test]
fn a_clean_run_reports_an_empty_unrunnable_array() {
    let (_dir, root) = fixture();
    let (code, report, stderr) = verify(&root, &["--only", "green"]);

    assert_eq!(code, Some(0), "stderr={stderr}");
    assert_eq!(report["ok"], true);
    assert_eq!(report["unrunnable"], serde_json::json!([]));
}
