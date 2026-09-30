//! Acceptance fixtures for `kind = "project-check"` sensors (#242).
//!
//! A project-check is a soft, project-specific facts check: declared purely in
//! `do-harness.toml`, warn-only unless `severity` says otherwise, and it
//! surfaces its `fix` hint with a failing verdict.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;
use tempfile::{TempDir, tempdir};

mod support;

use support::git_command;

/// Builds a `do-harness --root <root>` command using the real binary.
fn harness(root: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_do-harness"));
    command.arg("--root").arg(root);
    command
}

/// Runs a command, returning (exit code, `stdout`, `stderr`).
fn run(command: &mut Command) -> (Option<i32>, String, String) {
    let output = command.output().expect("run do-harness");
    (
        output.status.code(),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

/// A repository whose only sensor is a failing project-check, with `severity`
/// spliced into its declaration.
fn fixture_repo(severity: &str) -> (TempDir, PathBuf) {
    let dir = tempdir().unwrap();
    let root = dir.path().to_path_buf();
    let status = git_command(&root)
        .args(["init", "-q", "-b", "main"])
        .status()
        .expect("git init");
    assert!(status.success());
    std::fs::write(
        root.join("do-harness.toml"),
        format!(
            r#"language = "generic"

[signal-sets]
all = ["tracker-drift"]

[[sensors]]
name = "tracker-drift"
kind = "project-check"
argv = ["bash", "drift.sh"]
fix = "refresh the stale counts in STATUS.md"
{severity}
"#
        ),
    )
    .unwrap();
    std::fs::write(
        root.join("drift.sh"),
        "#!/usr/bin/env bash\necho 'drift: STATUS.md claims 13 open PRs, tracker reports 0'\necho 'FINDINGS: 1'\nexit 1\n",
    )
    .unwrap();
    (dir, root)
}

#[test]
fn project_check_warns_by_default_and_shows_the_fix_hint() {
    let (_dir, root) = fixture_repo("");
    let (code, stdout, stderr) =
        run(harness(&root).args(["verify", "--set", "all", "--format", "json"]));
    assert_eq!(
        code,
        Some(0),
        "a soft check must not fail the gate:\n{stdout}\n{stderr}"
    );
    let report: Value = serde_json::from_str(&stdout).expect("json report");
    let sensor = &report["sensors"][0];
    assert_eq!(sensor["name"], "tracker-drift");
    assert_eq!(sensor["ok"], serde_json::json!(false));
    assert_eq!(sensor["severity"], "warn");
    assert_eq!(sensor["allow_failure"], serde_json::json!(true));
    assert_eq!(sensor["fix"], "refresh the stale counts in STATUS.md");
    assert!(
        stderr.contains("FIX: refresh the stale counts in STATUS.md"),
        "the fix hint must be printed:\n{stderr}"
    );
    assert!(stderr.contains("WARN  tracker-drift"), "{stderr}");
}

#[test]
fn project_check_fails_the_run_under_strict() {
    let (_dir, root) = fixture_repo("");
    let (code, stdout, stderr) =
        run(harness(&root).args(["verify", "--set", "all", "--strict", "--format", "json"]));
    assert_eq!(
        code,
        Some(1),
        "strict must fail on the soft check:\n{stdout}\n{stderr}"
    );
}

#[test]
fn explicit_error_severity_overrides_the_soft_default() {
    let (_dir, root) = fixture_repo("severity = \"error\"");
    let (code, stdout, stderr) =
        run(harness(&root).args(["verify", "--set", "all", "--format", "json"]));
    assert_eq!(
        code,
        Some(1),
        "an explicit severity must win over the kind default:\n{stdout}\n{stderr}"
    );
    let report: Value = serde_json::from_str(&stdout).expect("json report");
    assert_eq!(report["sensors"][0]["severity"], "error");
    assert_eq!(
        report["sensors"][0]["allow_failure"],
        serde_json::json!(false)
    );
}
