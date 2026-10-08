//! Shared fixtures for unchanged-input reuse acceptance tests.
//!
//! Each fixture drives the real `do-harness` binary inside a temporary git
//! repository, with a `probe` sensor that appends to `runs.log` so every
//! execution is observable and reuse is provably "not spawned".

#![allow(clippy::unwrap_used, clippy::expect_used, dead_code)]

use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

use super::git_command;

/// Builds a `do-harness --root <root>` command using the real binary.
pub fn harness(root: &Path) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_do-harness"));
    cmd.arg("--root").arg(root);
    cmd.env_remove("DO_HARNESS_HOOK");
    cmd
}

/// Runs a harness command, returning (exit code, stdout, stderr).
pub fn run(cmd: &mut Command) -> (Option<i32>, String, String) {
    let output = cmd.output().expect("spawn do-harness");
    (
        output.status.code(),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

/// Runs a git command inside `root`, asserting success.
pub fn git(root: &Path, args: &[&str]) {
    let status = git_command(root)
        .args(args)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_AUTHOR_NAME", "t")
        .env("GIT_AUTHOR_EMAIL", "t@t")
        .env("GIT_COMMITTER_NAME", "t")
        .env("GIT_COMMITTER_EMAIL", "t@t")
        .status()
        .expect("spawn git");
    assert!(
        status.success(),
        "git {args:?} failed in {}",
        root.display()
    );
}

/// Current branch name in `root` (`git branch --show-current`).
pub fn git_branch(root: &Path) -> String {
    let output = git_command(root)
        .args(["branch", "--show-current"])
        .output()
        .expect("spawn git");
    assert!(output.status.success(), "git branch --show-current failed");
    String::from_utf8(output.stdout)
        .expect("branch name is utf8")
        .trim()
        .to_owned()
}

/// A `generic` config with a `verification` signal set over `set`, followed
/// by the given `[[sensors]]` blocks.
pub fn config_with_sensors(set: &[&str], sensor_blocks: &str) -> String {
    let names = set
        .iter()
        .map(|name| format!("\"{name}\""))
        .collect::<Vec<_>>()
        .join(", ");
    format!("language = \"generic\"\n\n[signal-sets]\nverification = [{names}]\n\n{sensor_blocks}")
}

/// A one-sensor `generic` config whose verification set is `["probe"]`.
pub fn probe_config(sensor_toml: &str) -> String {
    config_with_sensors(&["probe"], &format!("[[sensors]]\n{sensor_toml}"))
}

/// The standard caching probe: appends `hit` to `runs.log`, declares
/// `input.txt` as its only input.
pub const PROBE: &str = "name = \"probe\"\nargv = [\"sh\", \"-c\", \"printf hit >> runs.log\"]\ninputs = [\"input.txt\"]\n";

/// Creates a committed git repository with the given config plus an
/// `input.txt` and a non-input `other.txt` (both tracked).
pub fn fixture(config: &str) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_path_buf();
    std::fs::write(root.join("do-harness.toml"), config).unwrap();
    std::fs::write(root.join(".gitignore"), "runs.log\n").unwrap();
    std::fs::write(root.join("input.txt"), "v1\n").unwrap();
    std::fs::write(root.join("other.txt"), "v1\n").unwrap();
    git(&root, &["init", "-q"]);
    git(&root, &["add", "-A"]);
    git(&root, &["commit", "-qm", "test: base"]);
    (dir, root)
}

/// Creates a temporary directory that is deliberately not a git repository.
pub fn fixture_no_git(config: &str) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_path_buf();
    std::fs::write(root.join("do-harness.toml"), config).unwrap();
    std::fs::write(root.join("input.txt"), "v1\n").unwrap();
    (dir, root)
}

/// Runs `verify --set verification --format json` with extra arguments,
/// returning (exit code, parsed JSON, stderr).
pub fn verify(root: &Path, args: &[&str]) -> (Option<i32>, Value, String) {
    let (code, stdout, stderr) = verify_raw(root, args);
    let value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("verify stdout is not JSON ({e}):\n{stdout}\n{stderr}"));
    (code, value, stderr)
}

/// Runs `verify --set verification --format json` without parsing stdout, for
/// cases where the command is expected to fail before producing a report.
pub fn verify_raw(root: &Path, args: &[&str]) -> (Option<i32>, String, String) {
    let mut cmd = harness(root);
    cmd.args(["verify", "--set", "verification", "--format", "json"])
        .args(args);
    run(&mut cmd)
}

/// Runs `verify` expecting `code`, returning the parsed JSON report.
pub fn verify_code(root: &Path, args: &[&str], code: i32) -> (Value, String) {
    let (actual, value, stderr) = verify(root, args);
    assert_eq!(
        actual,
        Some(code),
        "verify {args:?} exited {actual:?}:\n{value}\n{stderr}"
    );
    (value, stderr)
}

/// `execution` field of the sensor at `index` in a verify report.
pub fn execution(value: &Value, index: usize) -> String {
    value["sensors"][index]["execution"]
        .as_str()
        .unwrap_or_else(|| panic!("sensor {index} has no execution field: {value}"))
        .to_owned()
}

/// Asserts the sensor at `index` ran in this run.
pub fn assert_ran(value: &Value, index: usize) {
    assert_eq!(
        execution(value, index),
        "ran",
        "expected a fresh run: {value}"
    );
}

/// Asserts the sensor at `index` reused a recorded beat, returning its id.
pub fn assert_reused(value: &Value, index: usize) -> i64 {
    assert_eq!(execution(value, index), "reused", "expected reuse: {value}");
    value["sensors"][index]["reused_beat_id"]
        .as_i64()
        .unwrap_or_else(|| panic!("reused sensor {index} has no beat id: {value}"))
}

/// Contents of the execution log (`""` when the sensor never ran).
pub fn log(root: &Path) -> String {
    std::fs::read_to_string(root.join("runs.log")).unwrap_or_default()
}

/// Reads the signal-set evidence artifact as JSON.
pub fn evidence(root: &Path) -> Value {
    let path = root.join(".do-harness/evidence.verification.json");
    serde_json::from_slice(
        &std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display())),
    )
    .expect("evidence json")
}

/// Adds a task through the CLI and returns its id.
pub fn task_add(root: &Path) -> i64 {
    let mut cmd = harness(root);
    cmd.args(["task", "add", "reuse probe"]);
    let (code, stdout, stderr) = run(&mut cmd);
    assert_eq!(code, Some(0), "task add failed:\n{stdout}\n{stderr}");
    stdout
        .split_whitespace()
        .nth(2)
        .and_then(|id| id.trim_end_matches(':').parse().ok())
        .unwrap_or_else(|| panic!("cannot parse task id from: {stdout}"))
}

/// Runs `status --set verification --format json`, returning (code, document).
pub fn status(root: &Path) -> (Option<i32>, Value) {
    let mut cmd = harness(root);
    cmd.args(["status", "--set", "verification", "--format", "json"]);
    let (code, stdout, stderr) = run(&mut cmd);
    let value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("status stdout is not JSON ({e}):\n{stdout}\n{stderr}"));
    (code, value)
}
