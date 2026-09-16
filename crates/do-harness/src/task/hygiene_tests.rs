#![allow(clippy::unwrap_used, clippy::expect_used)]

//! Lifecycle-hygiene tests: destructive task actions must not corrupt the
//! append-only event log, and `--dry-run` must be a genuine no-op.

use super::tests::write_catalog;
use super::*;
/// Regression: `task remove` used to run a bare `DELETE FROM tasks`, which
/// failed with `FOREIGN KEY constraint failed` for any task that had an event,
/// because `workflow_events.task_id` has no cascade. Refusing is the correct
/// outcome — the log is append-only and hash-chained, so deleting the events
/// would destroy audit evidence.
#[tokio::test(flavor = "current_thread")]
async fn remove_task_refuses_to_destroy_event_history() {
    let dir = tempfile::tempdir().unwrap();
    write_catalog(dir.path());
    let id = add_task(dir.path(), "removable", Some("mini"), None, None)
        .await
        .unwrap()
        .0;

    let err = remove_task(dir.path(), id).await.unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("append-only"), "got: {msg}");
    assert!(
        msg.contains("task fail"),
        "refusal must name the supported transition, got: {msg}"
    );

    // The task and its audit trail must both survive the refused removal.
    let conn = do_harness_db::connect_and_migrate(dir.path())
        .await
        .unwrap();
    assert!(do_harness_db::get_task(&conn, id).await.unwrap().is_some());
    let events = do_harness_db::list_all_events(&conn).await.unwrap();
    assert_eq!(events.len(), 1, "the TaskAdded event must survive");
}

/// A task with no event trail is an out-of-band orphan, which `doctor` already
/// flags; that row is removable because nothing references it.
#[tokio::test(flavor = "current_thread")]
async fn remove_task_deletes_an_orphan_row() {
    let dir = tempfile::tempdir().unwrap();
    let conn = do_harness_db::connect_and_migrate(dir.path())
        .await
        .unwrap();
    let now = do_harness_db::unix_now();
    conn.execute(
        "INSERT INTO tasks (title, method, subtask_index, status, created_at, updated_at) \
         VALUES ('orphan', NULL, 0, 'pending', ?1, ?1)",
        [now],
    )
    .await
    .unwrap();
    let orphan: i64 = {
        let mut rows = conn
            .query("SELECT id FROM tasks WHERE title = 'orphan'", ())
            .await
            .unwrap();
        rows.next().await.unwrap().unwrap().get(0).unwrap()
    };
    drop(conn);

    remove_task(dir.path(), orphan).await.unwrap();
    let conn = do_harness_db::connect_and_migrate(dir.path())
        .await
        .unwrap();
    assert!(
        do_harness_db::get_task(&conn, orphan)
            .await
            .unwrap()
            .is_none()
    );
}

/// Regression: `--dry-run` claimed "without side effects" but `task add` wrote
/// the row anyway. Exercises the guard `task_cmd` applies for every mutating
/// action.
#[tokio::test(flavor = "current_thread")]
async fn dry_run_add_writes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    write_catalog(dir.path());
    super::super::commands::task_cmd(
        dir.path(),
        super::super::cli::TaskAction::Add {
            title: "ghost".to_owned(),
            method: Some("mini".to_owned()),
            parent: None,
            precondition: None,
        },
        true,
    )
    .await
    .unwrap();

    let conn = do_harness_db::connect_and_migrate(dir.path())
        .await
        .unwrap();
    assert!(
        do_harness_db::list_tasks(&conn).await.unwrap().is_empty(),
        "dry-run must not create a task"
    );
    assert!(
        do_harness_db::list_all_events(&conn)
            .await
            .unwrap()
            .is_empty(),
        "dry-run must not append a workflow event"
    );
}
