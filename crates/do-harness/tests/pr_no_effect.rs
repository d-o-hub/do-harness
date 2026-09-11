//! Integration coverage for `do-harness pr no-effect` in local range mode.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::Path;
use std::process::Command;

use serde_json::Value;

fn git(dir: &Path, args: &[&str]) {
    let mut command = Command::new("git");
    command.current_dir(dir);
    for key in [
        "GIT_DIR",
        "GIT_WORK_TREE",
        "GIT_INDEX_FILE",
        "GIT_OBJECT_DIRECTORY",
        "GIT_ALTERNATE_OBJECT_DIRECTORIES",
        "GIT_CEILING_DIRECTORIES",
        "GIT_NAMESPACE",
        "GIT_PREFIX",
    ] {
        command.env_remove(key);
    }
    let output = command.args(args).output().expect("spawn git");
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn init_repo(dir: &Path) {
    git(dir, &["init", "-q", "-b", "main"]);
    git(dir, &["config", "user.email", "test@example.com"]);
    git(dir, &["config", "user.name", "Test"]);
}

fn commit_file(dir: &Path, name: &str, content: &str, message: &str) {
    std::fs::write(dir.join(name), content).unwrap();
    git(dir, &["add", name]);
    git(dir, &["commit", "-q", "-m", message]);
}

fn harness(root: &Path) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_do-harness"));
    cmd.arg("--root").arg(root);
    cmd
}

#[test]
fn range_mode_reports_effective_change() {
    let dir = tempfile::tempdir().unwrap();
    init_repo(dir.path());
    commit_file(dir.path(), "a.txt", "one\n", "base");
    git(dir.path(), &["switch", "-q", "-c", "feature"]);
    commit_file(dir.path(), "a.txt", "two\n", "change");

    let output = harness(dir.path())
        .args([
            "pr",
            "no-effect",
            "--base",
            "main",
            "--head",
            "feature",
            "--format",
            "json",
        ])
        .output()
        .expect("spawn do-harness");
    assert!(output.status.success(), "exit must be 0");
    let report: Value = serde_json::from_slice(&output.stdout).expect("json report");
    assert_eq!(report["effective_change"], serde_json::json!(true));
    assert_eq!(report["method"], serde_json::json!("git"));
    assert_eq!(report["mode"], serde_json::json!("range"));
}

#[test]
fn range_mode_reports_no_effect_for_empty_commit() {
    let dir = tempfile::tempdir().unwrap();
    init_repo(dir.path());
    commit_file(dir.path(), "a.txt", "one\n", "base");
    git(dir.path(), &["switch", "-q", "-c", "feature"]);
    git(
        dir.path(),
        &["commit", "-q", "--allow-empty", "-m", "empty"],
    );

    let output = harness(dir.path())
        .args([
            "pr",
            "no-effect",
            "--base",
            "main",
            "--head",
            "feature",
            "--format",
            "json",
        ])
        .output()
        .expect("spawn do-harness");
    assert!(output.status.success(), "exit must be 0");
    let report: Value = serde_json::from_slice(&output.stdout).expect("json report");
    assert_eq!(report["effective_change"], serde_json::json!(false));
}

#[test]
fn missing_target_is_a_usage_error() {
    let dir = tempfile::tempdir().unwrap();
    init_repo(dir.path());
    commit_file(dir.path(), "a.txt", "one\n", "base");

    let output = harness(dir.path())
        .args(["pr", "no-effect"])
        .output()
        .expect("spawn do-harness");
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("--base") && stderr.contains("--head"),
        "unexpected stderr: {stderr}"
    );
}

#[test]
fn mixed_pr_and_range_is_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let output = harness(dir.path())
        .args(["pr", "no-effect", "42", "--base", "main", "--head", "HEAD"])
        .output()
        .expect("spawn do-harness");
    assert_eq!(output.status.code(), Some(2));
}
