//! Fail-closed proofs for sensors that would otherwise pass vacuously:
//! AGENTS.md section 4, "a green sensor that executed zero checks is a failed
//! gate, not evidence".
//!
//! The `test` sensor runs `cargo nextest run` without `--no-tests=pass`, so a
//! suite that shrank to zero fails the gate instead of reporting green.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::Path;
use std::process::Command;

use serde_json::Value;

mod support;

/// Builds a `do-harness --root <root>` command using the real binary.
fn harness(root: &Path) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_do-harness"));
    support::isolate_command(&mut cmd);
    cmd.arg("--root").arg(root);
    cmd
}

/// Runs a harness command, returning (success, stdout, stderr).
fn run(cmd: &mut Command) -> (bool, String, String) {
    let output = cmd.output().expect("spawn do-harness");
    (
        output.status.success(),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

/// Whether the `test` sensor can run at all on this host.
fn nextest_available() -> bool {
    Command::new("cargo")
        .args(["nextest", "--version"])
        .output()
        .is_ok_and(|out| out.status.success())
}

/// The scaffold ships one unit test, so `verify --only test` is green; deleting
/// every test must flip it to a failure. Under `--no-tests=pass` the second
/// half of this proof is green too, which is the vacuous gate this test exists
/// to prevent.
#[test]
fn test_sensor_fails_when_the_workspace_has_no_tests() {
    let dir = tempfile::tempdir().unwrap();
    let status = support::git_command(dir.path())
        .args(["init", "-q"])
        .status()
        .expect("git init");
    assert!(status.success(), "git init failed");

    let (ok, out, err) = run(harness(dir.path()).arg("init"));
    assert!(ok, "init failed:\n{out}\n{err}");

    // A greenfield `do-harness.toml` delegates `test` to the built-in pack, so
    // the argv itself is pinned in `src/config/tests.rs` — a pin that holds on
    // hosts without cargo-nextest. What follows is the behaviour it protects.
    if !nextest_available() {
        eprintln!("SKIP: the zero-test proof needs cargo-nextest on PATH");
        return;
    }

    let (green, stdout, stderr) =
        run(harness(dir.path()).args(["verify", "--only", "test", "--format", "json"]));
    assert!(
        green,
        "the scaffolded suite must pass the test sensor:\nstdout: {stdout}\nstderr: {stderr}"
    );

    std::fs::write(
        dir.path().join("src/lib.rs"),
        "// The suite shrank to zero: nothing below is a test.\npub fn scaffold() -> u8 {\n    1\n}\n",
    )
    .unwrap();

    let (still_green, stdout, stderr) =
        run(harness(dir.path()).args(["verify", "--only", "test", "--format", "json"]));
    assert!(
        !still_green,
        "a workspace with zero tests must fail the test sensor:\nstdout: {stdout}\nstderr: {stderr}"
    );
    let report: Value = serde_json::from_str(&stdout).expect("verify json report");
    assert_eq!(report["ok"], Value::Bool(false));
    let failed = report["failed"].as_array().expect("failed array");
    assert!(
        failed.iter().any(|name| name == "test"),
        "the test sensor must be the sensor that failed: {failed:?}"
    );
}
