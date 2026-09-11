#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;

#[test]
fn identifier_allowlist_accepts_valid_and_rejects_injection() {
    assert!(is_identifier("tasks"));
    assert!(is_identifier("_beats"));
    assert!(is_identifier("col_1"));
    assert!(!is_identifier(""));
    assert!(!is_identifier("1tasks"));
    assert!(!is_identifier("tasks\""));
    assert!(!is_identifier("tasks; DROP"));
    assert!(!is_identifier("a-b"));
}

/// A malicious table identifier closes the gate as a failed assertion, not
/// by reaching the SQL string that would inject.
#[tokio::test(flavor = "current_thread")]
async fn db_assertion_rejects_sql_injection_identifier() {
    let dir = tempfile::tempdir().unwrap();
    let walk = WalkRun::absent();
    let grade = grade(
        dir.path(),
        "db:tasks\"; DROP TABLE beats; --:status=done:min=1",
        &walk,
    )
    .await
    .unwrap();
    assert!(!grade.passed);
    assert!(grade.reason.contains("invalid table/column identifier"));
}

/// A malicious column identifier closes the gate the same way the table
/// identifier does, even when it embeds a quote, semicolon, and comment.
#[tokio::test(flavor = "current_thread")]
async fn db_assertion_rejects_column_injection_payload() {
    let dir = tempfile::tempdir().unwrap();
    let walk = WalkRun::absent();
    for spec in [
        r#"db:tasks:status"; DROP -- =done:min=1"#,
        "db:tasks:=done:min=1",
    ] {
        let grade = grade(dir.path(), spec, &walk).await.unwrap();
        assert!(!grade.passed, "unexpected pass for {spec}");
        assert!(
            grade.reason.contains("invalid table/column identifier"),
            "expected allowlist rejection for {spec}: {}",
            grade.reason
        );
    }
}

/// Empty table and empty column names are rejected by the allowlist as a
/// clean failed grade, never a panic or a reach into the database.
#[tokio::test(flavor = "current_thread")]
async fn db_assertion_empty_identifiers_fail_cleanly() {
    let dir = tempfile::tempdir().unwrap();
    let walk = WalkRun::absent();
    for spec in ["db::status=done:min=1", "db:tasks:=done:min=1"] {
        let grade = grade(dir.path(), spec, &walk).await.unwrap();
        assert!(!grade.passed, "unexpected pass for {spec}");
        assert!(grade.reason.contains("invalid table/column identifier"));
    }
}

/// A fixture spec trying to move the sandbox `--root` (or `--config`) is
/// rejected as a failed grade instead of redirecting the child harness at
/// the caller's real workspace.
#[tokio::test(flavor = "current_thread")]
async fn cli_assertion_rejects_root_override() {
    let dir = tempfile::tempdir().unwrap();
    for argv in [
        "--root /tmp/elsewhere list",
        "--root=/tmp/elsewhere list",
        "list --root /tmp/elsewhere",
        "--config /tmp/evil.toml list",
        "--config=/tmp/evil.toml list",
    ] {
        let spec = format!("cli:{argv}:contains:anything");
        let grade = grade(dir.path(), &spec, &WalkRun::absent()).await.unwrap();
        assert!(!grade.passed, "unexpected pass for {spec}");
        assert!(
            grade.reason.contains("may not override"),
            "expected sandbox rejection for {spec}: {}",
            grade.reason
        );
    }
}

/// A walkthrough that was present but could not be launched fails its
/// `walk:` assertions with the cause surfaced, never a silent success.
#[tokio::test(flavor = "current_thread")]
async fn walk_assertion_surfaces_launch_failure_detail() {
    let walk = WalkRun {
        present: true,
        success: false,
        detail: Some("could not spawn sh: no /bin/sh".to_owned()),
    };
    let grade = grade(Path::new("/tmp"), "walk:", &walk).await.unwrap();
    assert!(!grade.passed);
    assert!(
        grade.reason.contains("could not spawn sh"),
        "expected launch detail in reason: {}",
        grade.reason
    );
}

/// Valid, allowlisted `db:` identifiers grade against a real temp database,
/// and a non-numeric `:min=` degrades gracefully to the default of 1.
#[tokio::test(flavor = "current_thread")]
async fn db_assertion_valid_identifiers_grade_against_real_db() {
    let dir = tempfile::tempdir().unwrap();
    let conn = do_harness_db::connect_and_migrate(dir.path())
        .await
        .unwrap();
    do_harness_db::record_sensor_outcome(
        &conn,
        &do_harness_db::NewBeat {
            task_id: None,
            beat_type: "sensor",
            status: "ok",
            sensor_exit_code: Some(0),
            sensor_name: Some("hardening-test"),
            started_at: 0,
            completed_at: Some(1),
        },
        true,
        None,
    )
    .await
    .unwrap();
    let walk = WalkRun::absent();

    let matched = grade(dir.path(), "db:beats:beat_type=sensor:min=1", &walk)
        .await
        .unwrap();
    assert!(matched.passed, "{}", matched.reason);

    let non_numeric_min = grade(dir.path(), "db:beats:beat_type=sensor:min=abc", &walk)
        .await
        .unwrap();
    assert!(
        non_numeric_min.passed,
        "non-numeric min must fall back to 1: {}",
        non_numeric_min.reason
    );

    let absent = grade(dir.path(), "db:beats:beat_type=missing:min=1", &walk)
        .await
        .unwrap();
    assert!(!absent.passed, "{}", absent.reason);
}
