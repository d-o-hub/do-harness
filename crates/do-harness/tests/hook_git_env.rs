//! Hook-inherited git state must not steer the harness or the scripts it
//! generates.
//!
//! Git exports `GIT_DIR` (and friends) to every hook. A generated script or a
//! fixture that follows it reads the caller's repository instead of the
//! workspace it was pointed at: `init`'s baseline verify goes RED and a local
//! `git push` fails for a reason unrelated to the pushed diff.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::{Path, PathBuf};
use std::process::Command;

mod support;

/// A bare `bash` is wrong on Windows: `PATH` finds the WSL launcher in
/// `System32`, which exits non-zero with an empty diagnostic when no
/// distribution is installed. `src/shell.rs` already resolves Git Bash there;
/// integration test targets cannot reach it (the package has no library
/// target), so the module is included by path.
#[path = "../src/shell.rs"]
#[allow(dead_code)]
mod shell;

fn isolated_command(program: &str) -> Command {
    let mut command = Command::new(program);
    support::isolate_command(&mut command);
    command
}

/// Builds a `do-harness --root <root>` command using the real binary.
fn harness(root: &Path) -> Command {
    let mut cmd = isolated_command(env!("CARGO_BIN_EXE_do-harness"));
    cmd.arg("--root").arg(root);
    cmd
}

/// Runs a harness command, returning (success, stdout).
fn run(cmd: &mut Command) -> (bool, String) {
    let output = cmd.output().expect("spawn do-harness");
    (
        output.status.success(),
        String::from_utf8_lossy(&output.stdout).into_owned(),
    )
}

/// A repository whose index tracks `packages/app/package.json`, like the
/// caller's repository when a hook runs it. Nothing it tracks exists under the
/// workspace being initialised.
fn foreign_repo_with_package_json() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_path_buf();
    std::fs::create_dir_all(root.join("packages/app")).unwrap();
    std::fs::write(
        root.join("packages/app/package.json"),
        "{\"name\":\"app\",\"version\":\"1.0.0\"}\n",
    )
    .unwrap();
    for args in [
        vec!["init", "-q"],
        vec!["add", "-A"],
        vec![
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@t",
            "commit",
            "-qm",
            "base",
        ],
    ] {
        let status = support::git_command(&root)
            .args(&args)
            .status()
            .expect("spawn git");
        assert!(status.success(), "git {args:?} failed");
    }
    (dir, root)
}

#[test]
fn rust_init_ignores_a_hook_inherited_git_dir() {
    let (_foreign_dir, foreign) = foreign_repo_with_package_json();
    let target = tempfile::tempdir().unwrap();

    let mut cmd = harness(target.path());
    cmd.env("GIT_DIR", foreign.join(".git"));
    let (ok, out) = run(cmd.arg("init"));
    assert!(
        ok,
        "init must ignore a hook-inherited GIT_DIR and read its own root:\n{out}"
    );
}

/// The generated scripts must be the ones doing the ignoring: run the release
/// preflight directly with the foreign git variables still set (the shipped
/// resolver picks the same bash the harness uses on Windows).
#[test]
fn generated_release_preflight_ignores_a_foreign_git_dir() {
    let (_foreign_dir, foreign) = foreign_repo_with_package_json();
    let target = tempfile::tempdir().unwrap();
    let (ok, out) = run(harness(target.path()).arg("init"));
    assert!(ok, "init failed:\n{out}");

    let mut bash = shell::bash();
    let output = support::isolate_command(&mut bash)
        .arg(target.path().join("scripts/check-release-preflight.sh"))
        .current_dir(target.path())
        .env("GIT_DIR", foreign.join(".git"))
        .env("GIT_COMMON_DIR", foreign.join(".git"))
        .output()
        .expect("spawn check-release-preflight.sh");
    assert!(
        output.status.success(),
        "check-release-preflight.sh must not follow a foreign git dir:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
