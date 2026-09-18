//! Shared fixtures for the DORA acceptance tests.
//!
//! Every fixture pins commit timestamps and injects the clock with `--now`, so
//! no test reads a real clock and the asserted numbers are exact.

#![allow(clippy::unwrap_used, clippy::expect_used, dead_code)]

use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

use super::git_command;

/// First fixture commit timestamp.
pub const T0: i64 = 1_700_000_000;

/// Builds a `do-harness --root <root>` command using the real binary.
pub fn harness(root: &Path) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_do-harness"));
    cmd.arg("--root").arg(root);
    cmd
}

/// Runs a harness command, returning (exit code, `stdout`, `stderr`).
pub fn run(cmd: &mut Command) -> (Option<i32>, String, String) {
    let output = cmd.output().expect("spawn do-harness");
    (
        output.status.code(),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

/// Runs a git command inside `root` with pinned identity, asserting success.
pub fn git(root: &Path, args: &[&str]) {
    let output = git_command(root)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_AUTHOR_NAME", "t")
        .env("GIT_AUTHOR_EMAIL", "t@t")
        .env("GIT_COMMITTER_NAME", "t")
        .env("GIT_COMMITTER_EMAIL", "t@t")
        .args(args)
        .output()
        .expect("spawn git");
    assert!(
        output.status.success(),
        "git {args:?} failed in {}:\n{}",
        root.display(),
        String::from_utf8_lossy(&output.stderr)
    );
}

/// Commits an empty change at `ts` with `subject`, pinned to an exact epoch.
pub fn commit_at(root: &Path, ts: i64, subject: &str) {
    let stamp = ts.to_string();
    let status = git_command(root)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_AUTHOR_NAME", "t")
        .env("GIT_AUTHOR_EMAIL", "t@t")
        .env("GIT_COMMITTER_NAME", "t")
        .env("GIT_COMMITTER_EMAIL", "t@t")
        .env("GIT_AUTHOR_DATE", &stamp)
        .env("GIT_COMMITTER_DATE", &stamp)
        .args(["commit", "-q", "--allow-empty", "-m", subject])
        .status()
        .expect("spawn git commit");
    assert!(
        status.success(),
        "git commit at {ts} failed in {}",
        root.display()
    );
}

/// Creates a fixture repository with the pinned DORA policy installed.
pub fn fixture_repo() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_path_buf();
    std::fs::create_dir_all(root.join("plans")).unwrap();
    std::fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../plans/dora.json"),
        root.join("plans/dora.json"),
    )
    .unwrap();
    git(&root, &["init", "-q"]);
    (dir, root)
}

/// Runs `dora --format json --now <now> --days 30`, asserting the exit code.
pub fn dora_json(root: &Path, now: i64, expect_code: i32) -> (String, String, Value) {
    let (code, stdout, stderr) = run(harness(root)
        .arg("dora")
        .arg("--format")
        .arg("json")
        .arg("--days")
        .arg("30")
        .arg("--now")
        .arg(now.to_string()));
    assert_eq!(
        code,
        Some(expect_code),
        "dora exited {code:?}:\n{stdout}\n{stderr}"
    );
    let value = serde_json::from_str(&stdout)
        .unwrap_or_else(|_| panic!("dora stdout is not JSON:\n{stdout}\n{stderr}"));
    (stdout, stderr, value)
}

/// Breach rule names in reported order.
pub fn breach_names(snapshot: &Value) -> Vec<String> {
    snapshot["breaches"]
        .as_array()
        .unwrap()
        .iter()
        .map(|breach| breach["name"].as_str().unwrap().to_string())
        .collect()
}

/// The fully populated fixture: two deploys, one revert, no restore.
pub fn deployed_fixture() -> (tempfile::TempDir, PathBuf) {
    let (dir, root) = fixture_repo();
    commit_at(&root, T0, "chore: seed");
    git(&root, &["tag", "v0.1.0"]);
    commit_at(&root, T0 + 100, "feat: a");
    commit_at(&root, T0 + 200, "feat: b");
    git(&root, &["tag", "v0.1.1"]);
    commit_at(&root, T0 + 400, "revert(fixture): drop the broken change");
    (dir, root)
}

/// Installs the repository's own `dora` sensor wiring into `root`.
///
/// The shim lives at `scripts/check-dora.sh` in the real repo and resolves the
/// binary through `DO_HARNESS_BIN`, so the fixture copies the committed shim
/// and points that variable at the test binary. The sensor spec mirrors
/// `do-harness.toml`, including `coverage-inputs`, because the policy
/// fingerprint is part of what the sensor tests exercise.
pub fn install_sensor(root: &Path) {
    std::fs::create_dir_all(root.join("scripts")).unwrap();
    std::fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scripts/check-dora.sh"),
        root.join("scripts/check-dora.sh"),
    )
    .unwrap();
    std::fs::write(
        root.join("do-harness.toml"),
        r#"
[signal-sets]
metrics = ["dora"]

[[sensors]]
name = "dora"
argv = ["bash", "scripts/check-dora.sh"]
severity = "warn"
coverage-inputs = ["plans/dora.json"]
"#,
    )
    .unwrap();
}

/// The fully populated fixture with the sensor wired in and committed.
pub fn installed_sensor_repo() -> (tempfile::TempDir, PathBuf) {
    let (dir, root) = deployed_fixture();
    install_sensor(&root);
    git(&root, &["add", "-A"]);
    git(&root, &["commit", "-qm", "chore: wire the dora sensor"]);
    (dir, root)
}

/// Runs `verify --only dora` with the sensor binary env applied.
pub fn verify_only_dora(root: &Path, extra: &[&str]) -> Command {
    let mut cmd = harness(root);
    cmd.args(["verify", "--only", "dora"]);
    cmd.args(extra);
    cmd.env("DO_HARNESS_BIN", env!("CARGO_BIN_EXE_do-harness"));
    cmd
}
