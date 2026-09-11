//! End-to-end hook installation: the managed `commit-msg` hook blocks a bad
//! subject through a real `git commit` and lets a good one through.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::Path;
use std::process::{Command, Output};

mod support;

use support::git_command;

fn harness() -> Command {
    Command::new(env!("CARGO_BIN_EXE_do-harness"))
}

fn run(cmd: &mut Command) -> Output {
    let out = cmd.output().expect("spawn");
    assert!(
        out.status.success(),
        "command failed: {:?}\nstdout: {}\nstderr: {}",
        cmd,
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    out
}

fn git(dir: &Path, args: &[&str]) -> Output {
    git_command(dir).args(args).output().expect("spawn git")
}

fn git_ok(dir: &Path, args: &[&str]) {
    let out = git(dir, args);
    assert!(
        out.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn installed_commit_msg_hook_blocks_bad_subject() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path();
    // `init` scaffolds a valid Rust crate at the root, so the pre-commit
    // `cargo fmt` sensor has something to format.
    run(harness().arg("--root").arg(root).arg("init"));

    git_ok(root, &["init", "-q"]);
    git_ok(root, &["config", "user.email", "e2e@example.test"]);
    git_ok(root, &["config", "user.name", "e2e"]);
    run(harness()
        .current_dir(root)
        .arg("--root")
        .arg(root)
        .arg("hook")
        .arg("install"));

    std::fs::write(root.join("note.txt"), "hello\n").expect("write note");
    git_ok(root, &["add", "-A"]);

    let bad = git_command(root)
        .args(["commit", "-m", "Bad Subject"])
        .env("DO_HARNESS_BIN", env!("CARGO_BIN_EXE_do-harness"))
        .output()
        .expect("spawn git commit");
    assert!(
        !bad.status.success(),
        "uppercase subject must be blocked by the commit-msg hook"
    );

    let good = git_command(root)
        .args(["commit", "-m", "feat: e2e hook check"])
        .env("DO_HARNESS_BIN", env!("CARGO_BIN_EXE_do-harness"))
        .output()
        .expect("spawn git commit");
    assert!(
        good.status.success(),
        "conventional subject must commit: {}",
        String::from_utf8_lossy(&good.stderr)
    );
}
