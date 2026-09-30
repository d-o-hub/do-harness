//! Truncation fixtures: the retained failure context, its anchor, and bounds.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;

/// Long outputs keep the first actionable failure and final summary, with
/// truncation explicitly marked.
#[test]
fn truncate_message_preserves_nextest_failure_and_summary() {
    let output = synthetic_nextest_output();
    let truncated = truncate_message(&output);

    assert!(truncated.chars().count() <= MAX_SIGNATURE_MESSAGE);
    assert!(truncated.contains("[output truncated;"));
    assert!(truncated.contains("[first failure context]"));
    assert!(truncated.contains("crate::tests::first_failed_test"));
    assert!(truncated.contains("stderr: assertion failed: expected value"));
    assert!(truncated.contains("[final output]"));
    assert!(truncated.contains("warning: 350/684 tests were not run due to test failure"));
    assert!(truncated.contains("FAIL: cargo llvm-cov nextest failed."));
    assert_eq!(truncate_message("short"), "short");
}

fn synthetic_nextest_output() -> String {
    let mut output =
        "nextest run started\nerror: wrapper summary precedes nextest details\n".to_owned();
    output.push_str(&"progress output\n".repeat(160));
    output.push_str("FAIL [ 0.001s] (333/684) crate::tests::first_failed_test\n");
    output.push_str("stderr: assertion failed: expected value\n");
    output.push_str(&"additional diagnostic context\n".repeat(180));
    output.push_str("warning: 350/684 tests were not run due to test failure\n");
    output.push_str("error: test run failed\nFAIL: cargo llvm-cov nextest failed.\n");
    output
}

/// A nextest failure survives telemetry persistence with its test identity,
/// stderr, explicit truncation marker, and final summary.
#[tokio::test(flavor = "current_thread")]
async fn record_verify_persists_actionable_nextest_failure_context() {
    let dir = tempfile::tempdir().unwrap();
    let report = VerifyReport {
        ok: false,
        root: dir.path().display().to_string(),
        failed: vec!["coverage".to_owned()],
        sensors: vec![SensorResult {
            fix: None,
            name: "coverage".to_owned(),
            ok: false,
            exit_code: Some(1),
            duration_ms: 1,
            severity: crate::config::SensorSeverity::Error,
            allow_failure: false,
            warned: false,
            findings: None,
            baseline: None,
            execution: crate::report::Execution::Ran,
            reused_beat_id: None,
            output: synthetic_nextest_output(),
        }],
        signal_set: None,
    };
    record_verify(
        dir.path(),
        &report,
        &[],
        &BeatScope::Global,
        &crate::config::rust_default(),
        None,
        &std::collections::BTreeMap::new(),
    )
    .await
    .unwrap();

    let conn = do_harness_db::connect_and_migrate(dir.path())
        .await
        .unwrap();
    let signature = do_harness_db::get_error_signature(&conn, "sensor:coverage", "global")
        .await
        .unwrap()
        .unwrap();
    let message = signature.message.unwrap();
    assert_eq!(signature.attempt_count, 1);
    assert!(message.contains("crate::tests::first_failed_test"));
    assert!(message.contains("stderr: assertion failed: expected value"));
    assert!(message.contains("[output truncated;"));
    assert!(message.contains("[final output]"));
    assert!(message.contains("warning: 350/684 tests were not run due to test failure"));
    assert!(message.contains("FAIL: cargo llvm-cov nextest failed."));
}
