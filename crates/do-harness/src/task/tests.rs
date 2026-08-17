#![allow(clippy::unwrap_used)]

use super::*;

/// Writes a minimal frozen method catalog for tests that gate on it.
fn write_catalog(root: &Path) {
    let dir = root.join("plans");
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("methods.json"),
        r#"{"methods":[
        {"name":"vertical-event-slice","subtasks":[
            {"name":"write-acceptance-test","sensor":"test","spike_candidate":false},
            {"name":"implement-slice","sensor":"test","spike_candidate":false},
            {"name":"verify-sensors","sensor":"check","spike_candidate":false}],
         "preconditions":[{"description":"understood"}]},
        {"name":"mini","subtasks":[
            {"name":"design","sensor":null,"spike_candidate":false},
            {"name":"verify","sensor":"test","spike_candidate":false}],
         "preconditions":[{"description":"mini"}]}
        ]}"#,
    )
    .unwrap();
}

/// Inserts an `"ok"` sensor beat scoped to `task_id` for the named sensor.
async fn insert_ok_beat(root: &Path, task_id: i64, sensor: &str) {
    let conn = do_harness_db::connect_and_migrate(root).await.unwrap();
    let now = do_harness_db::unix_now();
    do_harness_db::insert_beat(
        &conn,
        &do_harness_db::NewBeat {
            task_id: Some(task_id),
            beat_type: "sensor",
            status: "ok",
            sensor_exit_code: Some(0),
            sensor_name: Some(sensor),
            started_at: now,
            completed_at: Some(now),
        },
    )
    .await
    .unwrap();
}

#[tokio::test(flavor = "current_thread")]
async fn export_writes_task_snapshot() {
    let dir = tempfile::tempdir().unwrap();
    let conn = do_harness_db::connect_and_migrate(dir.path())
        .await
        .unwrap();
    do_harness_db::insert_task(
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

    let count = export_tasks(dir.path()).await.unwrap();

    assert_eq!(count, 1);
    let text = fs::read_to_string(dir.path().join("plans/tasks.json")).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(parsed["tasks"][0]["title"], "slice");
    assert_eq!(parsed["tasks"][0]["status"], "pending");
    assert_eq!(parsed["tasks"][0]["method"], "vertical-event-slice");
}

#[tokio::test(flavor = "current_thread")]
async fn export_writes_empty_snapshot_without_tasks() {
    let dir = tempfile::tempdir().unwrap();
    let count = export_tasks(dir.path()).await.unwrap();
    assert_eq!(count, 0);
    let text = fs::read_to_string(dir.path().join("plans/tasks.json")).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(parsed["tasks"].as_array().unwrap().len(), 0);
}

#[tokio::test(flavor = "current_thread")]
async fn add_task_inserts_pending_task() {
    let dir = tempfile::tempdir().unwrap();
    write_catalog(dir.path());
    let id = add_task(
        dir.path(),
        "implement workflow runtime",
        Some("vertical-event-slice"),
        None,
        Some("plans/tasks.json exists"),
    )
    .await
    .unwrap();

    let conn = do_harness_db::connect_and_migrate(dir.path())
        .await
        .unwrap();
    let task = do_harness_db::get_task(&conn, id).await.unwrap().unwrap();
    assert_eq!(task.status, do_harness_types::TaskState::Pending);
    assert_eq!(task.subtask_index, 0);
    assert_eq!(task.method.as_deref(), Some("vertical-event-slice"));
    assert_eq!(task.title, "implement workflow runtime");
    assert_eq!(task.parent_id, None);
}

#[tokio::test(flavor = "current_thread")]
async fn add_task_stores_parent_link() {
    let dir = tempfile::tempdir().unwrap();
    let parent = add_task(dir.path(), "parent", None, None, None)
        .await
        .unwrap();
    let child = add_task(dir.path(), "child", None, Some(parent), None)
        .await
        .unwrap();

    let conn = do_harness_db::connect_and_migrate(dir.path())
        .await
        .unwrap();
    let task = do_harness_db::get_task(&conn, child)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(task.parent_id, Some(parent));
}

#[tokio::test(flavor = "current_thread")]
async fn advance_task_increments_index_and_marks_in_progress() {
    let dir = tempfile::tempdir().unwrap();
    let id = add_task(dir.path(), "slice", None, None, None)
        .await
        .unwrap();

    let index = advance_task(dir.path(), id).await.unwrap();

    assert_eq!(index, 1);
    let conn = do_harness_db::connect_and_migrate(dir.path())
        .await
        .unwrap();
    let task = do_harness_db::get_task(&conn, id).await.unwrap().unwrap();
    assert_eq!(task.status, do_harness_types::TaskState::InProgress);
    assert_eq!(task.subtask_index, 1);
}

#[tokio::test(flavor = "current_thread")]
async fn advance_task_errors_for_missing_task() {
    let dir = tempfile::tempdir().unwrap();
    let err = advance_task(dir.path(), 999).await.unwrap_err();
    assert_eq!(err.to_string(), "task 999 not found");
}

#[tokio::test(flavor = "current_thread")]
async fn fail_task_marks_task_failed() {
    let dir = tempfile::tempdir().unwrap();
    let id = add_task(dir.path(), "slice", None, None, None)
        .await
        .unwrap();

    fail_task(dir.path(), id).await.unwrap();

    let conn = do_harness_db::connect_and_migrate(dir.path())
        .await
        .unwrap();
    let task = do_harness_db::get_task(&conn, id).await.unwrap().unwrap();
    assert_eq!(task.status, do_harness_types::TaskState::Failed);
}

#[tokio::test(flavor = "current_thread")]
async fn fail_task_errors_for_missing_task() {
    let dir = tempfile::tempdir().unwrap();
    let err = fail_task(dir.path(), 999).await.unwrap_err();
    assert_eq!(err.to_string(), "task 999 not found");
}

#[tokio::test(flavor = "current_thread")]
async fn add_task_rejects_unknown_method() {
    let dir = tempfile::tempdir().unwrap();
    write_catalog(dir.path());
    let err = add_task(dir.path(), "slice", Some("bogus-method"), None, None)
        .await
        .unwrap_err();
    assert_eq!(
        err.to_string(),
        "unknown method 'bogus-method': not in plans/methods.json"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn add_task_rejects_orphan_parent() {
    let dir = tempfile::tempdir().unwrap();
    write_catalog(dir.path());
    let err = add_task(dir.path(), "child", None, Some(999), None)
        .await
        .unwrap_err();
    assert_eq!(err.to_string(), "parent task 999 not found");
}

/// Advancing a sensor-gated subtask requires an `"ok"` beat for the task.
#[tokio::test(flavor = "current_thread")]
async fn advance_rejects_without_ok_beat() {
    let dir = tempfile::tempdir().unwrap();
    write_catalog(dir.path());
    let id = add_task(
        dir.path(),
        "slice",
        Some("vertical-event-slice"),
        None,
        None,
    )
    .await
    .unwrap();

    let err = advance_task(dir.path(), id).await.unwrap_err();
    assert!(
        err.to_string().contains("requires sensor 'test' to pass"),
        "unexpected error: {err}"
    );
}

/// A recorded `verify --record --task <id>` beat unlocks the gate.
#[tokio::test(flavor = "current_thread")]
async fn advance_allows_after_verify_record_beat() {
    let dir = tempfile::tempdir().unwrap();
    write_catalog(dir.path());
    let id = add_task(
        dir.path(),
        "slice",
        Some("vertical-event-slice"),
        None,
        None,
    )
    .await
    .unwrap();
    insert_ok_beat(dir.path(), id, "test").await;

    let index = advance_task(dir.path(), id).await.unwrap();
    assert_eq!(index, 1);
}

/// A task cannot be done while sensor-gated subtasks remain.
#[tokio::test(flavor = "current_thread")]
async fn done_rejects_before_last_subtask() {
    let dir = tempfile::tempdir().unwrap();
    write_catalog(dir.path());
    let id = add_task(dir.path(), "slice", Some("mini"), None, None)
        .await
        .unwrap();

    let err = done_task(dir.path(), id).await.unwrap_err();
    assert!(
        err.to_string().contains("subtasks remain"),
        "unexpected error: {err}"
    );
}

/// Advancing past all sensor-gated subtasks permits marking done.
#[tokio::test(flavor = "current_thread")]
async fn done_allows_after_advancing() {
    let dir = tempfile::tempdir().unwrap();
    write_catalog(dir.path());
    let id = add_task(dir.path(), "slice", Some("mini"), None, None)
        .await
        .unwrap();
    insert_ok_beat(dir.path(), id, "test").await;
    let _ = advance_task(dir.path(), id).await.unwrap();
    let _ = advance_task(dir.path(), id).await.unwrap();

    done_task(dir.path(), id).await.unwrap();

    let conn = do_harness_db::connect_and_migrate(dir.path())
        .await
        .unwrap();
    let task = do_harness_db::get_task(&conn, id).await.unwrap().unwrap();
    assert_eq!(task.status, TaskState::Done);
}

/// Advancing a `done` or `failed` task is rejected.
#[tokio::test(flavor = "current_thread")]
async fn advance_rejects_when_done_or_failed() {
    let dir = tempfile::tempdir().unwrap();
    write_catalog(dir.path());
    let done = add_task(dir.path(), "done", Some("mini"), None, None)
        .await
        .unwrap();
    insert_ok_beat(dir.path(), done, "test").await;
    let _ = advance_task(dir.path(), done).await.unwrap();
    let _ = advance_task(dir.path(), done).await.unwrap();
    done_task(dir.path(), done).await.unwrap();

    let err = advance_task(dir.path(), done).await.unwrap_err();
    assert_eq!(
        err.to_string(),
        format!("task {done} is done; cannot advance"),
        "unexpected error: {err}",
    );

    let failed = add_task(dir.path(), "failed", Some("mini"), None, None)
        .await
        .unwrap();
    fail_task(dir.path(), failed).await.unwrap();

    let err = advance_task(dir.path(), failed).await.unwrap_err();
    assert_eq!(
        err.to_string(),
        format!("task {failed} is failed; cannot advance"),
        "unexpected error: {err}",
    );
}

/// A task with no method cannot be marked done.
#[tokio::test(flavor = "current_thread")]
async fn done_rejects_task_without_method() {
    let dir = tempfile::tempdir().unwrap();
    write_catalog(dir.path());
    let id = add_task(dir.path(), "orphan", None, None, None)
        .await
        .unwrap();

    let err = done_task(dir.path(), id).await.unwrap_err();
    assert_eq!(
        err.to_string(),
        format!("task {id} has no method; cannot mark done")
    );
}

/// A passing beat for one sensor does not unlock a different sensor's gate.
///
/// Regression for the roast: `latest_sensor_beat_ok` previously accepted any
/// recent `"ok"` sensor beat, so a `fmt` pass could unlock a `check` gate.
#[tokio::test(flavor = "current_thread")]
async fn advance_requires_the_named_sensor_beat() {
    let dir = tempfile::tempdir().unwrap();
    write_catalog(dir.path());
    let id = add_task(
        dir.path(),
        "slice",
        Some("vertical-event-slice"),
        None,
        None,
    )
    .await
    .unwrap();

    // An ok beat for the WRONG sensor must not satisfy the `test` gate.
    insert_ok_beat(dir.path(), id, "fmt").await;
    let err = advance_task(dir.path(), id).await.unwrap_err();
    assert!(
        err.to_string().contains("requires sensor 'test' to pass"),
        "expected test-gate error, got: {err}"
    );

    // The right sensor's ok beat unlocks the gate.
    insert_ok_beat(dir.path(), id, "test").await;
    advance_task(dir.path(), id).await.unwrap();
}

/// Advancing the `check`-gated subtask requires a `check` beat, not a `test` one.
#[tokio::test(flavor = "current_thread")]
async fn check_gate_is_not_satisfied_by_a_test_beat() {
    let dir = tempfile::tempdir().unwrap();
    write_catalog(dir.path());
    let id = add_task(
        dir.path(),
        "slice",
        Some("vertical-event-slice"),
        None,
        None,
    )
    .await
    .unwrap();

    // Pass both `test` gates to reach index 2 (the `check`-gated subtask).
    insert_ok_beat(dir.path(), id, "test").await;
    advance_task(dir.path(), id).await.unwrap();
    insert_ok_beat(dir.path(), id, "test").await;
    advance_task(dir.path(), id).await.unwrap();

    // index=2 -> verify-sensors gates on `check`; a `test` beat is not enough.
    insert_ok_beat(dir.path(), id, "test").await;
    let err = advance_task(dir.path(), id).await.unwrap_err();
    assert!(
        err.to_string().contains("requires sensor 'check' to pass"),
        "expected check-gate error, got: {err}"
    );

    insert_ok_beat(dir.path(), id, "check").await;
    advance_task(dir.path(), id).await.unwrap();
}
