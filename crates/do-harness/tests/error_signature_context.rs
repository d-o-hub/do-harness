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
