//! Shared helpers for the hook end-to-end test crate.

use std::path::Path;
use std::process::{Command, Output};

use crate::support::git_command;

pub(super) fn harness() -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_do-harness"));
    crate::support::isolate_command(&mut cmd);
    cmd
}

pub(super) fn run(cmd: &mut Command) -> Output {
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

pub(super) fn git(dir: &Path, args: &[&str]) -> Output {
    git_command(dir).args(args).output().expect("spawn git")
}

pub(super) fn git_ok(dir: &Path, args: &[&str]) {
    let out = git(dir, args);
    assert!(
        out.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// A hook-running git command: pinned to the binary under test and isolated
/// from the cargo session that started the hook.
///
/// The managed hooks run `do-harness verify`, which re-enters the cargo session
/// that started the hook: under `cargo llvm-cov` the scaffold's `coverage`
/// sensor runs a second `cargo llvm-cov nextest` and the inherited
/// `RUSTC_WRAPPER`/target dir make it rebuild into the outer instrumented
/// workspace, exhausting memory (measured 2026-10-20: anonymous memory passed
/// 7 GB within seconds). Isolate the hook exactly as `harness()` does.
pub(super) fn git_with_bin(dir: &Path, bin: &str) -> Command {
    let mut command = git_command(dir);
    crate::support::isolate_command(&mut command);
    command.env("DO_HARNESS_BIN", bin);
    command
}

/// Runs git with the harness binary pinned, so the managed hooks execute the
/// binary under test instead of building a release one.
pub(super) fn git_ok_with_bin(dir: &Path, args: &[&str], bin: &str) {
    let out = git_with_bin(dir, bin)
        .args(args)
        .output()
        .expect("spawn git");
    assert!(
        out.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

pub(super) fn push_with_bin(dir: &Path, bin: &str) -> Output {
    git_with_bin(dir, bin)
        .args(["push", "origin", "HEAD:refs/heads/main"])
        .output()
        .expect("spawn git push")
}
