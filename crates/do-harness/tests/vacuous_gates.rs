//! Fail-closed proofs for sensors that would otherwise pass vacuously:
//! AGENTS.md section 4, "a green sensor that executed zero checks is a failed
//! gate, not evidence".
//!
//! The generated `test` sensor runs `scripts/check-test.sh`, which prefers
//! `cargo nextest run` (no `--no-tests=pass`) and falls back to `cargo test`
//! behind its own zero-test guard, so a suite that shrank to zero fails the
//! gate instead of reporting green on either path.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::process::Command;

use serde_json::Value;

mod support;

use support::harness;

/// Runs a harness command, returning (success, stdout, stderr).
fn run(cmd: &mut Command) -> (bool, String, String) {
    let output = cmd.output().expect("spawn do-harness");
    (
        output.status.success(),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
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

    // The generated `do-harness.toml` points `test` at `scripts/check-test.sh`,
    // which guards the zero-test case on both the nextest and `cargo test`
    // paths, so this proof runs on every host with cargo.
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
