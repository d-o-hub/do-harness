//! Real-CLI acceptance fixtures for `do-harness task context`.
//!
//! Each case drives the built binary over a disposable git worktree and state
//! database, proving the documented contract: bounded stable output, explicit
//! unknowns, task-scoped freshness, and a read-only projection.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;

mod support;

use support::{git, harness, run};

const METHODS: &str = r#"{"methods":[
    {"name":"vertical-event-slice","subtasks":[
        {"name":"write-acceptance-test","sensor":"test","spike_candidate":false},
        {"name":"implement-slice","sensor":"test","spike_candidate":false}],
     "preconditions":[{"description":"understood"}]}
]}"#;

/// A committed generic-language repo whose `test` sensor passes.
fn repo(test_argv: &str) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_path_buf();
    fs::create_dir_all(root.join("plans")).unwrap();
    fs::write(root.join("plans/methods.json"), METHODS).unwrap();
    fs::write(
        root.join("do-harness.toml"),
        format!(
            "language = \"generic\"\n\n[[sensors]]\nname = \"test\"\nargv = [\"{test_argv}\"]\n\n\
             [[sensors]]\nname = \"check\"\nargv = [\"true\"]\n"
        ),
    )
    .unwrap();
    fs::write(root.join(".gitignore"), ".do-harness/\n").unwrap();
    git(&root, &["init", "-q"]);
    git(&root, &["add", "."]);
    git(&root, &["commit", "-qm", "init"]);
    (dir, root)
}

fn add_task(root: &Path, title: &str) -> i64 {
    let output = harness(root)
        .args(["task", "add", title, "--method", "vertical-event-slice"])
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let mut list = harness(root);
    list.args(["task", "list", "--format", "json"]);
    let (code, stdout, stderr) = run(&mut list);
    assert_eq!(code, Some(0), "stderr: {stderr}");
    let value: Value = serde_json::from_str(&stdout).unwrap();
    let id = value["tasks"][0]["id"].as_i64().unwrap();
    assert!(id > 0);
    id
}

#[test]
fn json_reports_step_next_action_and_explicit_unknowns() {
    let (_dir, root) = repo("true");
    let id = add_task(&root, "build the projection");
    let db = root.join(".do-harness/agent_state.db");
    let before = fs::read(&db).unwrap();

    let mut cmd = harness(&root);
    cmd.args(["task", "context", &id.to_string(), "--format", "json"]);
    let (code, stdout, stderr) = run(&mut cmd);
    assert_eq!(code, Some(0), "stderr: {stderr}");
    let value: Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(value["schema_version"], 1);
    assert_eq!(value["task"]["id"], id);
    assert_eq!(value["step"]["sensor"], "test");
    assert_eq!(value["acceptance_criteria"]["available"], false);
    assert_eq!(value["acceptance_criteria"]["reason"], "not_recorded");
    assert_eq!(value["next_action"]["kind"], "verify");
    assert_eq!(value["workspace"]["scope"], "workspace");
    assert!(
        value["unknowns"]
            .as_array()
            .unwrap()
            .iter()
            .any(|u| u["section"] == "acceptance_criteria")
    );
    assert_eq!(
        fs::read(&db).unwrap(),
        before,
        "context must not write the database"
    );
    assert!(!root.join(".do-harness/evidence.json").exists());
}

#[test]
fn recorded_pass_then_context_reports_green_and_advance() {
    let (_dir, root) = repo("true");
    let id = add_task(&root, "green path");
    let mut verify = harness(&root);
    verify.args([
        "verify",
        "--only",
        "test",
        "--record",
        "--task",
        &id.to_string(),
    ]);
    let (code, stdout, stderr) = run(&mut verify);
    assert_eq!(code, Some(0), "stdout: {stdout}\nstderr: {stderr}");

    let mut cmd = harness(&root);
    cmd.args(["task", "context", &id.to_string(), "--format", "json"]);
    let (code, stdout, stderr) = run(&mut cmd);
    assert_eq!(code, Some(0), "stderr: {stderr}");
    let value: Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(value["gating"]["state"], "green");
    assert_eq!(value["next_action"]["kind"], "advance");
}

#[test]
fn failing_sensor_then_context_reports_a_blocker() {
    let (_dir, root) = repo("false");
    let id = add_task(&root, "red path");
    let mut verify = harness(&root);
    verify.args([
        "verify",
        "--only",
        "test",
        "--record",
        "--task",
        &id.to_string(),
    ]);
    let (code, _, stderr) = run(&mut verify);
    assert_eq!(code, Some(1), "stderr: {stderr}");

    let mut cmd = harness(&root);
    cmd.args(["task", "context", &id.to_string(), "--format", "json"]);
    let (code, stdout, stderr) = run(&mut cmd);
    assert_eq!(code, Some(0), "stderr: {stderr}");
    let value: Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(value["gating"]["state"], "red");
    assert_eq!(value["gating"]["reason"], "failures");
    assert_eq!(value["next_action"]["kind"], "verify");
    assert_eq!(value["next_action"]["reason"], "sensor_failed");
    assert_eq!(value["failures"]["items"][0]["signature"], "sensor:test");
}

#[test]
fn text_output_is_bounded_and_deterministic() {
    let (_dir, root) = repo("true");
    let id = add_task(&root, "stable output");
    let id = id.to_string();

    let mut first = harness(&root);
    first.args(["task", "context", &id]);
    let (code, stdout, stderr) = run(&mut first);
    assert_eq!(code, Some(0), "stderr: {stderr}");
    let mut second = harness(&root);
    second.args(["task", "context", &id]);
    let (code, again, stderr) = run(&mut second);
    assert_eq!(code, Some(0), "stderr: {stderr}");
    assert_eq!(stdout, again, "same inputs must render the same text");

    let mut tiny = harness(&root);
    tiny.args(["task", "context", &id, "--max-bytes", "200"]);
    let (code, bounded, stderr) = run(&mut tiny);
    assert_eq!(code, Some(0), "stderr: {stderr}");
    assert!(bounded.contains(&format!("task {id}:")), "{bounded}");
    assert!(bounded.contains("next:"), "{bounded}");
}

#[test]
fn missing_database_is_documented_and_not_created() {
    let (_dir, root) = repo("true");
    let mut cmd = harness(&root);
    cmd.args(["task", "context", "1", "--format", "json"]);
    let (code, stdout, stderr) = run(&mut cmd);
    // An unavailable state store is a configuration problem (exit 2), and the
    // documented diagnostic is still printed first.
    assert_eq!(code, Some(2), "stderr: {stderr}");
    let value: Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(value["database"]["state"], "missing");
    assert_eq!(value["next_action"]["kind"], "unavailable");
    assert!(!root.join(".do-harness/agent_state.db").exists());
}
