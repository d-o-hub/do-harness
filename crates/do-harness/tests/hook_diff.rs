//! End-to-end coverage for `do-harness hook diff`.
//!
//! `hook diff` compares each installed managed hook against the body that
//! `hook install` would write *now*, so it catches a stale or hand-edited hook
//! that the marker-only check in `hook status` cannot. Before this fixture the
//! command was covered only by a source-level string match in `hooks/tests.rs`;
//! these tests drive the real binary against a scratch repository.
//!
//! Exit codes: `0` when every installed hook matches its template, and `2` when
//! one differs or is missing (the `Usage` class of `CliError`).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::{Path, PathBuf};
use std::process::Command;

use tempfile::{TempDir, tempdir};

mod support;

/// stdout when every installed hook matches the template its config implies.
const MATCHING_STDOUT: &str = "Hooks match installed templates.";
/// stdout header when at least one installed hook differs or is missing.
const DIFFERENT_STDOUT: &str = "One or more hooks differ or are missing:";
/// stderr tail the failing run ends with.
const OUT_OF_DATE_STDERR: &str = "managed hooks are out of date";
/// `hook diff` on a fully matching tree exits `0`.
const MATCH_EXIT: i32 = 0;
/// A drifted or missing managed hook is a usage/configuration error (exit `2`).
const DRIFT_EXIT: i32 = 2;

/// Builds a `do-harness --root <root>` command with session state isolated.
fn harness(root: &Path) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_do-harness"));
    support::isolate_command(&mut cmd);
    cmd.arg("--root").arg(root);
    cmd
}

/// Runs a harness command, returning (exit code, `stdout`, `stderr`).
fn run(cmd: &mut Command) -> (Option<i32>, String, String) {
    let out = cmd.output().expect("spawn do-harness");
    (
        out.status.code(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

fn hook_diff(root: &Path) -> (Option<i32>, String, String) {
    run(harness(root).args(["hook", "diff"]))
}

/// A `do-harness.toml` whose `pre-commit` hook runs exactly `pre_commit`
/// sensors; editing that list changes the script `hook install` would write.
fn config(pre_commit: &[&str]) -> String {
    let list = pre_commit
        .iter()
        .map(|name| format!("\"{name}\""))
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "language = \"generic\"\n\n\
         [hooks]\n\
         pre-commit = [{list}]\n\
         pre-push = []\n\n\
         [[sensors]]\n\
         name = \"fmt\"\n\
         argv = [\"true\"]\n\n\
         [[sensors]]\n\
         name = \"loc\"\n\
         argv = [\"true\"]\n"
    )
}

/// A git repo carrying a tracked `do-harness.toml` and freshly installed hooks.
fn installed_repo() -> (TempDir, PathBuf) {
    let dir = tempdir().unwrap();
    let root = dir.path().to_path_buf();
    let status = support::git_command(&root)
        .args(["init", "-q", "-b", "main"])
        .status()
        .expect("spawn git init");
    assert!(status.success(), "git init failed in {}", root.display());
    let cfg_status = support::git_command(&root)
        .args(["config", "core.hooksPath", ".git/hooks"])
        .status()
        .expect("spawn git config");
    assert!(cfg_status.success());
    std::fs::write(root.join("do-harness.toml"), config(&["fmt"])).unwrap();

    let (code, stdout, stderr) = run(harness(&root).args(["hook", "install"]));
    assert_eq!(code, Some(0), "hook install failed:\n{stdout}\n{stderr}");
    (dir, root)
}

/// The negative control (installed hooks match) and the positive case (a
/// tracked-config change makes the installed hook stale) in one fixture, so a
/// diff that always passed and one that always failed are both caught.
#[test]
fn matching_hooks_pass_and_a_changed_template_names_the_hook() {
    let (_dir, root) = installed_repo();

    // Right after install nothing has drifted: the command must succeed quietly.
    let (code, stdout, stderr) = hook_diff(&root);
    assert_eq!(
        code,
        Some(MATCH_EXIT),
        "matching hooks must exit 0:\n{stdout}\n{stderr}"
    );
    assert!(stdout.contains(MATCHING_STDOUT), "stdout: {stdout}");
    assert!(!stdout.contains(DIFFERENT_STDOUT), "stdout: {stdout}");

    // Change the tracked config so the `pre-commit` template gains a sensor.
    // Without reinstalling, the file on disk no longer matches what install
    // would write, which is exactly the drift the command exists to report.
    std::fs::write(root.join("do-harness.toml"), config(&["fmt", "loc"])).unwrap();

    let (code, stdout, stderr) = hook_diff(&root);
    assert_eq!(
        code,
        Some(DRIFT_EXIT),
        "a stale hook must exit 2:\n{stdout}\n{stderr}"
    );
    assert!(stdout.contains(DIFFERENT_STDOUT), "stdout: {stdout}");
    assert!(
        stdout.contains("pre-commit:"),
        "the drift must name the stale hook:\n{stdout}"
    );
    assert!(
        stdout.contains("first difference at line"),
        "the report must locate the first differing line:\n{stdout}"
    );
    // The hooks whose config did not change stay out of the report.
    assert!(!stdout.contains("pre-push:"), "stdout: {stdout}");
    assert!(!stdout.contains("commit-msg:"), "stdout: {stdout}");
    assert!(stderr.contains(OUT_OF_DATE_STDERR), "stderr: {stderr}");
}

/// A deleted managed hook takes the missing branch rather than the differing
/// one, and is named as missing.
#[test]
fn a_missing_hook_is_named_and_fails() {
    let (_dir, root) = installed_repo();
    std::fs::remove_file(root.join(".git/hooks/pre-push")).unwrap();

    let (code, stdout, stderr) = hook_diff(&root);
    assert_eq!(
        code,
        Some(DRIFT_EXIT),
        "a missing hook must exit 2:\n{stdout}\n{stderr}"
    );
    assert!(
        stdout.contains("  pre-push: missing"),
        "the report must name the missing hook:\n{stdout}"
    );
    assert!(
        !stdout.contains("pre-commit:"),
        "hooks that still match stay quiet:\n{stdout}"
    );
    assert!(stderr.contains(OUT_OF_DATE_STDERR), "stderr: {stderr}");
}
