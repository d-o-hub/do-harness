#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;
use crate::report::SensorResult;

#[tokio::test(flavor = "current_thread")]
async fn record_verify_persists_beats_and_signatures() {
    let dir = tempfile::tempdir().unwrap();
    let report = VerifyReport {
        ok: false,
        root: dir.path().display().to_string(),
        failed: vec!["fail".to_owned()],
        sensors: vec![SensorResult {
            name: "fail".to_owned(),
            ok: false,
            exit_code: Some(1),
            duration_ms: 1,
            severity: crate::config::SensorSeverity::Error,
            allow_failure: false,
            warned: false,
            findings: None,
            baseline: None,
            output: "boom".to_owned(),
        }],
        signal_set: None,
    };

    record_verify(dir.path(), &report, &[], None).await.unwrap();

    let conn = do_harness_db::connect_and_migrate(dir.path())
        .await
        .unwrap();
    let beats = do_harness_db::list_beats(&conn, None).await.unwrap();
    assert_eq!(beats.len(), 1);
    assert_eq!(beats[0].beat_type, "sensor");
    assert_eq!(beats[0].status, "failed");
    assert_eq!(beats[0].sensor_exit_code, Some(1));
    let sig = do_harness_db::get_error_signature(&conn, "sensor:fail", None)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(sig.attempt_count, 1);
    assert_eq!(sig.message.as_deref(), Some("boom"));
}

#[tokio::test(flavor = "current_thread")]
async fn record_verify_skips_signatures_when_all_pass() {
    let dir = tempfile::tempdir().unwrap();
    let report = VerifyReport {
        ok: true,
        root: dir.path().display().to_string(),
        failed: vec![],
        sensors: vec![SensorResult {
            name: "fmt".to_owned(),
            ok: true,
            exit_code: Some(0),
            duration_ms: 1,
            severity: crate::config::SensorSeverity::Error,
            allow_failure: false,
            warned: false,
            findings: None,
            baseline: None,
            output: String::new(),
        }],
        signal_set: None,
    };

    record_verify(dir.path(), &report, &[], None).await.unwrap();

    let conn = do_harness_db::connect_and_migrate(dir.path())
        .await
        .unwrap();
    let beats = do_harness_db::list_beats(&conn, None).await.unwrap();
    assert_eq!(beats.len(), 1);
    assert_eq!(beats[0].status, "ok");
    assert!(
        do_harness_db::get_error_signature(&conn, "sensor:fmt", None)
            .await
            .unwrap()
            .is_none()
    );
}

/// A blocked sensor is skipped entirely: no beat, no signature bump.
#[tokio::test(flavor = "current_thread")]
async fn record_verify_skips_blocked_sensor_signature() {
    let dir = tempfile::tempdir().unwrap();
    let report = VerifyReport {
        ok: false,
        root: dir.path().display().to_string(),
        failed: vec!["halted".to_owned()],
        sensors: vec![SensorResult {
            name: "halted".to_owned(),
            ok: false,
            exit_code: None,
            duration_ms: 0,
            severity: crate::config::SensorSeverity::Error,
            allow_failure: false,
            warned: false,
            findings: None,
            baseline: None,
            output: "halted: ...".to_owned(),
        }],
        signal_set: None,
    };

    record_verify(dir.path(), &report, &["halted".to_owned()], None)
        .await
        .unwrap();

    let conn = do_harness_db::connect_and_migrate(dir.path())
        .await
        .unwrap();
    let beats = do_harness_db::list_beats(&conn, None).await.unwrap();
    assert!(beats.is_empty());
    assert!(
        do_harness_db::get_error_signature(&conn, "sensor:halted", None)
            .await
            .unwrap()
            .is_none()
    );
}

/// `struck_sensors` halts names at the strike threshold and ignores the rest.
#[tokio::test(flavor = "current_thread")]
async fn struck_sensors_halts_only_struck_out_names() {
    let dir = tempfile::tempdir().unwrap();
    let conn = do_harness_db::connect_and_migrate(dir.path())
        .await
        .unwrap();
    for _ in 0..FAIL_FAST_STRIKES {
        do_harness_db::bump_error_signature(&conn, "sensor:struck", None, Some("boom"))
            .await
            .unwrap();
    }
    for _ in 0..FAIL_FAST_STRIKES - 1 {
        do_harness_db::bump_error_signature(&conn, "sensor:close", None, Some("boom"))
            .await
            .unwrap();
    }

    let names = vec!["struck".to_owned(), "close".to_owned(), "fresh".to_owned()];
    let blocked = struck_sensors(dir.path(), &names, None).await.unwrap();
    assert_eq!(blocked, vec!["struck".to_owned()]);
    assert!(
        struck_sensors(dir.path(), &[], None)
            .await
            .unwrap()
            .is_empty()
    );
}

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
            name: "coverage".to_owned(),
            ok: false,
            exit_code: Some(1),
            duration_ms: 1,
            severity: crate::config::SensorSeverity::Error,
            allow_failure: false,
            warned: false,
            findings: None,
            baseline: None,
            output: synthetic_nextest_output(),
        }],
        signal_set: None,
    };
    record_verify(dir.path(), &report, &[], None).await.unwrap();

    let conn = do_harness_db::connect_and_migrate(dir.path())
        .await
        .unwrap();
    let signature = do_harness_db::get_error_signature(&conn, "sensor:coverage", None)
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

/// A passing sensor resets its strike counter so fail-fast recovers.
#[tokio::test(flavor = "current_thread")]
async fn record_verify_resets_strikes_on_pass() {
    let dir = tempfile::tempdir().unwrap();
    let conn = do_harness_db::connect_and_migrate(dir.path())
        .await
        .unwrap();
    for _ in 0..FAIL_FAST_STRIKES {
        do_harness_db::bump_error_signature(&conn, "sensor:fmt", None, Some("boom"))
            .await
            .unwrap();
    }
    drop(conn);

    let report = VerifyReport {
        ok: true,
        root: dir.path().display().to_string(),
        failed: vec![],
        sensors: vec![SensorResult {
            name: "fmt".to_owned(),
            ok: true,
            exit_code: Some(0),
            duration_ms: 1,
            severity: crate::config::SensorSeverity::Error,
            allow_failure: false,
            warned: false,
            findings: None,
            baseline: None,
            output: String::new(),
        }],
        signal_set: None,
    };
    record_verify(dir.path(), &report, &[], None).await.unwrap();

    let conn = do_harness_db::connect_and_migrate(dir.path())
        .await
        .unwrap();
    assert!(
        do_harness_db::get_error_signature(&conn, "sensor:fmt", None)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        struck_sensors(dir.path(), &["fmt".to_owned()], None)
            .await
            .unwrap()
            .is_empty()
    );
}

/// Beats and signatures are scoped to the active task when one is set.
#[tokio::test(flavor = "current_thread")]
async fn record_verify_scopes_to_task() {
    let dir = tempfile::tempdir().unwrap();
    let conn = do_harness_db::connect_and_migrate(dir.path())
        .await
        .unwrap();
    let (task_id, _) = do_harness_db::insert_task_with_event(
        &conn,
        &do_harness_db::NewTask {
            title: "slice",
            method: Some("vertical-event-slice"),
            subtask_index: 0,
            precondition: None,
            parent_id: None,
        },
    )
    .await
    .unwrap();
    drop(conn);

    let report = VerifyReport {
        ok: false,
        root: dir.path().display().to_string(),
        failed: vec!["check".to_owned()],
        sensors: vec![SensorResult {
            name: "check".to_owned(),
            ok: false,
            exit_code: Some(1),
            duration_ms: 1,
            severity: crate::config::SensorSeverity::Error,
            allow_failure: false,
            warned: false,
            findings: None,
            baseline: None,
            output: "E0308".to_owned(),
        }],
        signal_set: None,
    };
    record_verify(dir.path(), &report, &[], Some(task_id))
        .await
        .unwrap();

    let conn = do_harness_db::connect_and_migrate(dir.path())
        .await
        .unwrap();
    assert_eq!(
        do_harness_db::list_beats(&conn, Some(task_id))
            .await
            .unwrap()
            .len(),
        1
    );
    assert!(
        do_harness_db::get_error_signature(&conn, "sensor:check", None)
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(
        do_harness_db::get_error_signature(&conn, "sensor:check", Some(task_id))
            .await
            .unwrap()
            .unwrap()
            .attempt_count,
        1
    );
}
