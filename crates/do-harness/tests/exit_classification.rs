//! Exit-code classification for pre-check failures.
//!
//! The contract has one rule: a failure that stops a command before any check
//! runs is a usage/discovery problem (exit 2), while a check that ran and
//! failed is exit 1. `doctor` and `eval` used to report both as 1, so the same
//! `find_git_dir` failure exited 2 from `hook status` and 1 from `doctor`.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::{Path, PathBuf};
use std::process::Command;

/// A configured root that is deliberately not a git repository.
struct Fixture {
    _temp: tempfile::TempDir,
    root: PathBuf,
}

fn fixture() -> Fixture {
    let temp = tempfile::tempdir().expect("tempdir");
    let root = temp.path().join("root");
    std::fs::create_dir_all(&root).expect("create root");
    std::fs::write(
        root.join("do-harness.toml"),
        "# test fixture policy\n[[sensors]]\nname = \"noop\"\nargv = [\"true\"]\nwhen-changed = [\"**/*\"]\n",
    )
    .expect("write config");
    Fixture { _temp: temp, root }
}

/// Whether `dir` resolves to a working tree, i.e. whether `find_git_dir` would
/// find one above it.
fn inside_git_repo(dir: &Path) -> bool {
    Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

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

/// A root that is not a git repository stops `doctor` before any check runs:
/// exit 2, agreeing with what `hook status` already reports for the same
/// failure.
#[test]
fn doctor_without_a_git_repository_is_a_usage_error() {
    let fx = fixture();
    assert!(
        !inside_git_repo(&fx.root),
        "precondition: {} must not be inside a git repository",
        fx.root.display()
    );

    let (code, stdout, stderr) = run(harness(&fx.root).arg("doctor"));
    assert_eq!(
        code,
        Some(2),
        "doctor outside a git repository must exit 2\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(
        stderr.to_lowercase().contains("git"),
        "stderr must name the git problem\nstderr: {stderr}"
    );

    let (hook_code, _, hook_stderr) = run(harness(&fx.root).args(["hook", "status"]));
    assert_eq!(
        hook_code,
        Some(2),
        "hook status must keep exiting 2 for the same failure\nstderr: {hook_stderr}"
    );
}

/// An unknown skill name is the operator's input, not a failed eval: exit 2.
#[test]
fn eval_with_an_unknown_skill_is_a_usage_error() {
    let fx = fixture();
    let (code, stdout, stderr) = run(harness(&fx.root).args(["eval", "--skill", "nope"]));
    assert_eq!(
        code,
        Some(2),
        "eval --skill nope must exit 2\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(
        stderr.contains("not found under"),
        "stderr must name the missing skill\nstderr: {stderr}"
    );
}

/// An unreachable state database stops `eval` before any eval runs: exit 2,
/// the class the other persistence commands report.
#[test]
fn eval_with_an_unopenable_state_database_is_a_usage_error() {
    let fx = fixture();
    // A regular file where the state directory must be, so connecting cannot
    // succeed. `eval` has no skills to discover here, so the database is the
    // first thing that can fail.
    std::fs::write(fx.root.join(".do-harness"), "not a directory").expect("block the state dir");

    let (code, stdout, stderr) = run(harness(&fx.root).arg("eval"));
    assert_eq!(
        code,
        Some(2),
        "eval with an unopenable state database must exit 2\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(
        stderr.contains(".do-harness"),
        "stderr must name the state path\nstderr: {stderr}"
    );
}
