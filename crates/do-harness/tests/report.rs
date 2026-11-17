//! Acceptance fixtures for `do-harness report`.
//!
//! The command exports recorded beats, so the fixture records a real verify run
//! first: a report that only reads seeded rows would not prove the shapes agree
//! with what `verify --record` actually persists.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::Path;

use serde_json::Value;

mod support;

use support::{harness, run};

const CONFIG: &str = r#"
[signal-sets]
all = ["ok", "soft"]

[[sensors]]
name = "ok"
argv = ["true"]

[[sensors]]
name = "soft"
argv = ["sh", "-c", "exit 1"]
severity = "warn"
"#;

fn fixture() -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_path_buf();
    std::fs::write(root.join("do-harness.toml"), CONFIG).unwrap();
    (dir, root)
}

fn record_a_run(root: &Path) {
    let (code, _stdout, stderr) = run(harness(root)
        .arg("verify")
        .arg("--set")
        .arg("all")
        .arg("--record")
        .arg("--global"));
    assert_eq!(code, Some(0), "recording run must pass:\n{stderr}");
}

#[test]
fn json_exports_every_recorded_execution() {
    let (_dir, root) = fixture();
    record_a_run(&root);

    let (code, stdout, stderr) = run(harness(&root)
        .arg("report")
        .arg("--format")
        .arg("json")
        .arg("--all"));
    assert_eq!(code, Some(0), "{stderr}");
    let document: Value = serde_json::from_str(&stdout).expect("report json");
    assert_eq!(document["schema_version"], serde_json::json!(1));
    assert_eq!(document["scope"], serde_json::json!("all"));
    assert!(document["window"]["since"].is_null());
    let executions = document["executions"].as_array().expect("executions");
    let mut names: Vec<&str> = executions
        .iter()
        .map(|row| row["sensor"].as_str().expect("sensor"))
        .collect();
    names.sort_unstable();
    assert_eq!(names, vec!["ok", "soft"]);
    for row in executions {
        assert_eq!(row["scope"], serde_json::json!("global"));
        assert!(row["started_at"].as_i64().unwrap() > 0);
        assert!(
            row["duration_ms"].as_i64().unwrap() >= 0,
            "duration must be derived, not null: {row}"
        );
    }
    let soft = executions
        .iter()
        .find(|row| row["sensor"] == "soft")
        .expect("soft row");
    assert_eq!(soft["status"], serde_json::json!("warn"));
    assert_eq!(soft["exit_code"], serde_json::json!(1));
}

#[test]
fn prometheus_output_is_valid_text_exposition() {
    let (_dir, root) = fixture();
    record_a_run(&root);

    let (code, stdout, stderr) = run(harness(&root)
        .arg("report")
        .arg("--format")
        .arg("prometheus")
        .arg("--all"));
    assert_eq!(code, Some(0), "{stderr}");
    assert!(
        stdout.contains("# TYPE do_harness_sensor_runs_total counter"),
        "{stdout}"
    );
    assert!(
        stdout.contains("do_harness_sensor_failures_total{sensor=\"soft\",scope=\"global\"} 1"),
        "advisory failures are still non-ok outcomes: {stdout}"
    );
    assert!(
        stdout.contains("do_harness_sensor_duration_seconds_total{sensor=\"ok\",scope=\"global\"}"),
        "{stdout}"
    );
    // One series per (sensor, scope), never a duplicate sample.
    let runs: Vec<&str> = stdout
        .lines()
        .filter(|line| line.starts_with("do_harness_sensor_runs_total{"))
        .collect();
    assert_eq!(runs.len(), 2, "{stdout}");
}

#[test]
fn the_window_filters_and_validates() {
    let (_dir, root) = fixture();
    record_a_run(&root);

    let (code, stdout, _stderr) = run(harness(&root)
        .arg("report")
        .arg("--format")
        .arg("json")
        .arg("--since")
        .arg("0")
        .arg("--until")
        .arg("1")
        .arg("--all"));
    assert_eq!(code, Some(0));
    let document: Value = serde_json::from_str(&stdout).expect("report json");
    assert_eq!(document["executions"], serde_json::json!([]));

    let (code, _stdout, stderr) = run(harness(&root)
        .arg("report")
        .arg("--since")
        .arg("100")
        .arg("--until")
        .arg("50"));
    assert_eq!(code, Some(2), "an inverted window is a usage error");
    assert!(stderr.contains("--until"), "{stderr}");

    let (code, _stdout, stderr) = run(harness(&root).arg("report").arg("--since").arg("soon"));
    assert_eq!(code, Some(2));
    assert!(stderr.contains("Unix timestamp"), "{stderr}");
}

#[test]
fn an_unknown_branch_scope_reports_nothing() {
    let (_dir, root) = fixture();
    record_a_run(&root);

    let (code, stdout, _stderr) = run(harness(&root)
        .arg("report")
        .arg("--format")
        .arg("json")
        .arg("--branch")
        .arg("some-other-branch"));
    assert_eq!(code, Some(0));
    let document: Value = serde_json::from_str(&stdout).expect("report json");
    assert_eq!(document["executions"], serde_json::json!([]));
    assert_eq!(
        document["scope"],
        serde_json::json!("branch:some-other-branch")
    );
}
