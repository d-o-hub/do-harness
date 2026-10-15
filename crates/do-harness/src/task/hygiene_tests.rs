#![allow(clippy::unwrap_used, clippy::expect_used)]

//! Lifecycle-hygiene tests: destructive task actions must not corrupt the
//! append-only event log, and `--dry-run` must be a genuine no-op.

use std::fs;

use super::tests::write_catalog;
use super::*;
use crate::report::Format;
use do_harness_types::TaskState;

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
    crate::commands::task_cmd(
        dir.path(),
        crate::cli::TaskAction::Add {
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

/// Regression: `task export --dry-run` wrote `plans/tasks.json` before any
/// dry-run check ran, so a routine dry run could overwrite the committed
/// snapshot. The guard now short-circuits ahead of the write.
#[tokio::test(flavor = "current_thread")]
async fn dry_run_export_writes_no_file() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir_all(dir.path().join("plans")).unwrap();
    let snapshot = dir.path().join("plans/tasks.json");
    fs::write(&snapshot, "{\"sentinel\":true}").unwrap();

    crate::commands::task_cmd(
        dir.path(),
        crate::cli::TaskAction::Export {
            output: None,
            stdout: false,
            format: Format::Json,
        },
        true,
    )
    .await
    .unwrap();

    assert_eq!(
        fs::read_to_string(&snapshot).unwrap(),
        "{\"sentinel\":true}",
        "dry-run export must not touch the snapshot"
    );
}

/// `--dry-run` must be a faithful preview: the same call without it does write,
/// so the guard is a real gate rather than a no-op that hides a broken path.
#[tokio::test(flavor = "current_thread")]
async fn export_without_dry_run_does_write() {
    let dir = tempfile::tempdir().unwrap();
    write_catalog(dir.path());
    add_task(dir.path(), "real", Some("mini"), None, None)
        .await
        .unwrap();

    crate::commands::task_cmd(
        dir.path(),
        crate::cli::TaskAction::Export {
            output: None,
            stdout: false,
            format: Format::Json,
        },
        false,
    )
    .await
    .unwrap();

    let written = fs::read_to_string(dir.path().join("plans/tasks.json")).unwrap();
    assert!(written.contains("\"real\""), "export must write the board");
}

/// Regression: `task done --dry-run` reported success before any gate ran, so
/// it previewed a transition the real call refuses instead of surfacing it.
#[tokio::test(flavor = "current_thread")]
async fn dry_run_done_surfaces_the_same_refusal() {
    let dir = tempfile::tempdir().unwrap();
    write_catalog(dir.path());
    let (id, _) = add_task(dir.path(), "gated", Some("mini"), None, None)
        .await
        .unwrap();

    let dry = crate::commands::task_cmd(
        dir.path(),
        crate::cli::TaskAction::Done { id, dry_run: false },
        true,
    )
    .await
    .unwrap_err()
    .to_string();
    let real = done_task(dir.path(), id).await.unwrap_err().to_string();
    assert_eq!(dry, real, "a dry run must preview the real refusal");
    assert!(dry.contains("subtasks remain"), "unexpected: {dry}");
}

/// `--dry-run` on a method-less `task done` previews success without writing.
#[tokio::test(flavor = "current_thread")]
async fn dry_run_done_methodless_task_writes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    write_catalog(dir.path());
    let (id, _) = add_task(dir.path(), "methodless", None, None, None)
        .await
        .unwrap();

    crate::commands::task_cmd(
        dir.path(),
        crate::cli::TaskAction::Done { id, dry_run: true },
        true,
    )
    .await
    .unwrap();

    let conn = do_harness_db::connect_and_migrate(dir.path())
        .await
        .unwrap();
    let task = do_harness_db::get_task(&conn, id).await.unwrap().unwrap();
    assert_eq!(
        task.status,
        TaskState::Pending,
        "dry run must not change status"
    );
}

/// Regression: a dry-run advance past a sensor-gated subtask printed success
/// even though the real transition refuses without a passing beat.
#[tokio::test(flavor = "current_thread")]
async fn dry_run_advance_surfaces_the_unsatisfied_gate() {
    let dir = tempfile::tempdir().unwrap();
    write_catalog(dir.path());
    let (id, _) = add_task(
        dir.path(),
        "gated",
        Some("vertical-event-slice"),
        None,
        None,
    )
    .await
    .unwrap();

    let dry = crate::commands::task_cmd(
        dir.path(),
        crate::cli::TaskAction::Advance { id, dry_run: false },
        true,
    )
    .await
    .unwrap_err()
    .to_string();
    let real = advance_task(dir.path(), id).await.unwrap_err().to_string();
    assert_eq!(dry, real, "a dry run must preview the real refusal");
    assert!(
        dry.contains("requires sensor 'test' to pass"),
        "unexpected: {dry}"
    );
}

/// A dry run that does pass its gate still writes nothing: the subtask pointer
/// and the append-only event log stay untouched.
#[tokio::test(flavor = "current_thread")]
async fn dry_run_advance_past_a_free_subtask_writes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    write_catalog(dir.path());
    let (id, _) = add_task(dir.path(), "free", Some("mini"), None, None)
        .await
        .unwrap();
    let before = {
        let conn = do_harness_db::connect_and_migrate(dir.path())
            .await
            .unwrap();
        do_harness_db::list_all_events(&conn).await.unwrap().len()
    };

    crate::commands::task_cmd(
        dir.path(),
        crate::cli::TaskAction::Advance { id, dry_run: false },
        true,
    )
    .await
    .unwrap();

    let conn = do_harness_db::connect_and_migrate(dir.path())
        .await
        .unwrap();
    let task = do_harness_db::get_task(&conn, id).await.unwrap().unwrap();
    assert_eq!(task.subtask_index, 0, "dry run must not move the pointer");
    assert_eq!(
        do_harness_db::list_all_events(&conn).await.unwrap().len(),
        before,
        "dry run must not append a workflow event"
    );
}
