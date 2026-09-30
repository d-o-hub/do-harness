//! End-to-end coverage for bounded sensor failure details in `errors list`.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::Path;
use std::process::{Command, Output};

use serde_json::Value;
use tempfile::tempdir;

fn harness(root: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_do-harness"));
    command.arg("--root").arg(root);
    command
}

fn run(command: &mut Command) -> Output {
    command.output().expect("run do-harness")
}

#[test]
fn errors_list_retains_nextest_failure_and_ordinary_failure_details() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    std::fs::write(
        root.join("do-harness.toml"),
        r#"[[sensors]]
name = "coverage"
argv = ["bash", "coverage-failure.sh"]

[[sensors]]
name = "ordinary"
argv = ["bash", "ordinary-failure.sh"]
"#,
    )
    .unwrap();
    std::fs::write(
        root.join("coverage-failure.sh"),
        r#"#!/usr/bin/env bash
printf 'error: wrapper summary precedes nextest details\n'
for i in {1..160}; do printf 'progress %s\n' "$i"; done
printf 'FAIL [ 0.001s] (333/684) crate::tests::first_failed_test\n'
for i in {1..180}; do printf 'additional diagnostic context\n'; done
printf 'warning: 350/684 tests were not run due to test failure\n'
printf 'error: test run failed\n'
printf 'FAIL: cargo llvm-cov nextest failed.\n'
printf 'stderr: assertion failed: expected value\n' >&2
exit 1
"#,
    )
    .unwrap();
    std::fs::write(
        root.join("ordinary-failure.sh"),
        "#!/usr/bin/env bash\nprintf 'ordinary failure text\\n'\nexit 1\n",
    )
    .unwrap();

    let verify = run(harness(root).args([
        "verify", "--record", "--format", "json", "--only", "coverage", "--only", "ordinary",
    ]));
    assert_eq!(
        verify.status.code(),
        Some(1),
        "verify must fail:\n{}",
        String::from_utf8_lossy(&verify.stdout)
    );

    let listed = run(harness(root).args(["errors", "list", "--format", "json"]));
    assert!(
        listed.status.success(),
        "errors list failed:\n{}\n{}",
        String::from_utf8_lossy(&listed.stdout),
        String::from_utf8_lossy(&listed.stderr)
    );
    let signatures: Value = serde_json::from_slice(&listed.stdout).expect("errors list JSON");
    let rows = signatures.as_array().expect("signature list array");
    let coverage = rows
        .iter()
        .find(|row| row["signature"] == "sensor:coverage")
        .expect("coverage signature recorded");
    let message = coverage["message"].as_str().expect("coverage message");
    assert!(message.contains("[output truncated;"));
    assert!(message.contains("[first failure context]"));
    assert!(message.contains("crate::tests::first_failed_test"));
    assert!(message.contains("stderr: assertion failed: expected value"));
    assert!(message.contains("[final output]"));
    assert!(message.contains("warning: 350/684 tests were not run due to test failure"));
    assert!(message.contains("FAIL: cargo llvm-cov nextest failed."));

    let ordinary = rows
        .iter()
        .find(|row| row["signature"] == "sensor:ordinary")
        .expect("ordinary signature recorded");
    let ordinary_message = ordinary["message"].as_str().expect("ordinary message");
    assert!(ordinary_message.contains("ordinary failure text"));
    assert!(!ordinary_message.contains("[output truncated;"));
}

/// nextest prints its exec-failure header as a bare `execfail` token between
/// rule characters; that line must still anchor the retained context (a colon
/// is not part of the real header).
#[test]
fn bare_execfail_header_still_anchors_the_context() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    std::fs::write(
        root.join("do-harness.toml"),
        r#"[[sensors]]
name = "exec"
argv = ["bash", "exec-failure.sh"]
"#,
    )
    .unwrap();
    std::fs::write(
        root.join("exec-failure.sh"),
        r#"#!/usr/bin/env bash
printf '        ──────── execfail ────────\n'
for i in {1..1900}; do printf 'progress %s\n' "$i"; done
printf '   Canceling due to test failure\n'
for i in {1..60}; do printf 'trailing noise line %s\n' "$i"; done
exit 1
"#,
    )
    .unwrap();

    let verify = run(harness(root).args(["verify", "--record", "--only", "exec"]));
    assert!(!verify.status.success(), "the sensor must fail");
    let list = run(harness(root).args(["errors", "list", "--format", "json"]));
    let rows: Vec<Value> = serde_json::from_slice(&list.stdout).expect("signature JSON");
    let message = rows
        .iter()
        .find(|row| row["signature"] == "sensor:exec")
        .and_then(|row| row["message"].as_str())
        .expect("exec signature recorded");

    assert!(
        message.contains("[first failure context]"),
        "the exec-failure header must anchor the context: {message}"
    );
    assert!(message.contains("execfail"), "{message}");
}
#[test]
fn passing_execfail_named_line_does_not_anchor_the_context() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    std::fs::write(
        root.join("do-harness.toml"),
        r#"[[sensors]]
name = "log"
argv = ["bash", "log-failure.sh"]
"#,
    )
    .unwrap();
    std::fs::write(
        root.join("log-failure.sh"),
        r#"#!/usr/bin/env bash
printf '        PASS [   0.001s] (1/684) crate::tests::handles_execfail_semantics\n'
for i in {1..1900}; do printf 'progress %s\n' "$i"; done
printf '        FAIL [   0.002s] (2/684) crate::tests::the_real_failure\n'
printf 'assertion `left == right` failed: the actionable text\n'
for i in {1..60}; do printf 'trailing noise line %s\n' "$i"; done
exit 1
"#,
    )
    .unwrap();

    let verify = run(harness(root).args(["verify", "--record", "--only", "log"]));
    assert!(!verify.status.success(), "the sensor must fail");
    let list = run(harness(root).args(["errors", "list", "--format", "json"]));
    assert!(list.status.success(), "errors list must succeed");
    let rows: Vec<Value> = serde_json::from_slice(&list.stdout).expect("signature JSON");
    let message = rows
        .iter()
        .find(|row| row["signature"] == "sensor:log")
        .and_then(|row| row["message"].as_str())
        .expect("log signature recorded");

    assert!(
        message.contains("the_real_failure") || message.contains("the actionable text"),
        "the real failure must be retained: {message}"
    );
    assert!(
        !message.contains("handles_execfail_semantics"),
        "a passing test name must not anchor the context: {message}"
    );
}
