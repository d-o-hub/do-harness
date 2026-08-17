#![allow(clippy::unwrap_used)]

use super::*;
use do_harness_types::DomainEvent;

use super::tests::{insert_ok_beat, write_catalog};

#[tokio::test(flavor = "current_thread")]
async fn add_task_returns_task_added_event() {
    let dir = tempfile::tempdir().unwrap();
    write_catalog(dir.path());
    let (id, event) = add_task(
        dir.path(),
        "implement workflow runtime",
        Some("vertical-event-slice"),
        None,
        None,
    )
    .await
    .unwrap();

    assert_eq!(event.name(), "TaskAdded");
    match event {
        WorkflowEvent::TaskAdded(TaskAdded {
            id: got,
            title,
            method,
            ..
        }) => {
            assert_eq!(got, id);
            assert_eq!(title, "implement workflow runtime");
            assert_eq!(method.as_deref(), Some("vertical-event-slice"));
        }
        other => panic!("expected TaskAdded, got {}", other.name()),
    }
}

#[tokio::test(flavor = "current_thread")]
async fn advance_task_returns_task_advanced_event() {
    let dir = tempfile::tempdir().unwrap();
    write_catalog(dir.path());
    let (id, _added) = add_task(dir.path(), "slice", Some("mini"), None, None)
        .await
        .unwrap();

    let (index, event) = advance_task(dir.path(), id).await.unwrap();

    assert_eq!(index, 1);
    assert_eq!(event.name(), "TaskAdvanced");
    match event {
        WorkflowEvent::TaskAdvanced(TaskAdvanced {
            id: got,
            subtask_index,
        }) => {
            assert_eq!(got, id);
            assert_eq!(subtask_index, 1);
        }
        other => panic!("expected TaskAdvanced, got {}", other.name()),
    }
}

#[tokio::test(flavor = "current_thread")]
async fn done_task_returns_task_completed_event() {
    let dir = tempfile::tempdir().unwrap();
    write_catalog(dir.path());
    let (id, _added) = add_task(dir.path(), "slice", Some("mini"), None, None)
        .await
        .unwrap();
    insert_ok_beat(dir.path(), id, "test").await;
    let _ = advance_task(dir.path(), id).await.unwrap();
    let _ = advance_task(dir.path(), id).await.unwrap();

    let event = done_task(dir.path(), id).await.unwrap();

    assert_eq!(event, WorkflowEvent::TaskCompleted(TaskCompleted { id }));
}

#[tokio::test(flavor = "current_thread")]
async fn fail_task_returns_task_failed_event() {
    let dir = tempfile::tempdir().unwrap();
    let (id, _added) = add_task(dir.path(), "slice", None, None, None)
        .await
        .unwrap();

    let event = fail_task(dir.path(), id).await.unwrap();

    assert_eq!(event, WorkflowEvent::TaskFailed(TaskFailed { id }));
}

#[tokio::test(flavor = "current_thread")]
async fn list_tasks_folds_board_and_summary() {
    let dir = tempfile::tempdir().unwrap();
    write_catalog(dir.path());
    let (alpha, _a) = add_task(dir.path(), "alpha", Some("mini"), None, None)
        .await
        .unwrap();
    let (beta, _b) = add_task(dir.path(), "beta", None, None, None)
        .await
        .unwrap();
    insert_ok_beat(dir.path(), alpha, "test").await;
    let _ = advance_task(dir.path(), alpha).await.unwrap();
    let _ = advance_task(dir.path(), alpha).await.unwrap();
    done_task(dir.path(), alpha).await.unwrap();
    fail_task(dir.path(), beta).await.unwrap();

    let conn = do_harness_db::connect_and_migrate(dir.path())
        .await
        .unwrap();
    let tasks = do_harness_db::list_tasks(&conn).await.unwrap();
    let mut board = TaskBoard::new();
    for task in &tasks {
        board.apply(&status_event(task)).unwrap();
    }
    assert_eq!(board.to_counts(), (0, 0, 1, 1));
    assert_eq!(
        format!(
            "summary: pending={} in_progress={} done={} failed={}",
            board.pending, board.in_progress, board.done, board.failed
        ),
        "summary: pending=0 in_progress=0 done=1 failed=1"
    );
}
