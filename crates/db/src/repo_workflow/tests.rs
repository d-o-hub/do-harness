#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;
use crate::migrate::connect_and_migrate;

fn new_task(title: &str) -> NewTask<'_> {
    NewTask {
        title,
        method: Some("mini"),
        subtask_index: 0,
        precondition: None,
        parent_id: None,
    }
}

#[tokio::test(flavor = "current_thread")]
async fn insert_task_with_event_roundtrips_via_list() {
    let dir = tempfile::tempdir().unwrap();
    let conn = connect_and_migrate(dir.path()).await.unwrap();

    let (id, event) = insert_task_with_event(&conn, &new_task("slice"))
        .await
        .unwrap();
    drop(conn);

    let conn = connect_and_migrate(dir.path()).await.unwrap();
    let events = list_all_events(&conn).await.unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].0, id);
    assert_eq!(events[0].1, event);
    assert_eq!(events[0].1.name(), "TaskAdded");
}

#[tokio::test(flavor = "current_thread")]
async fn command_sequence_persists_full_event_stream() {
    let dir = tempfile::tempdir().unwrap();
    let conn = connect_and_migrate(dir.path()).await.unwrap();

    let (id, _) = insert_task_with_event(&conn, &new_task("slice"))
        .await
        .unwrap();
    let (index, _) = advance_subtask_with_event(&conn, id, None).await.unwrap();
    assert_eq!(index, 1);
    update_task_status_with_event(&conn, id, TaskState::Done, &[])
        .await
        .unwrap();
    drop(conn);

    let conn = connect_and_migrate(dir.path()).await.unwrap();
    let kinds: Vec<&'static str> = list_all_events(&conn)
        .await
        .unwrap()
        .into_iter()
        .map(|(_, event)| event.name())
        .collect();
    assert_eq!(kinds, vec!["TaskAdded", "TaskAdvanced", "TaskCompleted"]);
}

#[tokio::test(flavor = "current_thread")]
async fn terminal_writer_rejects_non_terminal_states() {
    let dir = tempfile::tempdir().unwrap();
    let conn = connect_and_migrate(dir.path()).await.unwrap();
    let (id, _) = insert_task_with_event(&conn, &new_task("slice"))
        .await
        .unwrap();

    for state in [TaskState::Pending, TaskState::InProgress] {
        let err = update_task_status_with_event(&conn, id, state, &[])
            .await
            .unwrap_err();
        assert!(matches!(err, DbError::InvalidTerminalState(_)));
    }
}

#[tokio::test(flavor = "current_thread")]
async fn advance_subtask_reports_missing_task_as_not_found() {
    let dir = tempfile::tempdir().unwrap();
    let conn = connect_and_migrate(dir.path()).await.unwrap();

    let err = advance_subtask_with_event(&conn, 999, None)
        .await
        .unwrap_err();
    assert!(matches!(err, DbError::NotFound(_)));
}

/// The terminal writer checks task existence inside its transaction, so a
/// missing id surfaces as `NotFound` instead of a raw constraint error.
#[tokio::test(flavor = "current_thread")]
async fn terminal_writer_reports_missing_task_as_not_found() {
    let dir = tempfile::tempdir().unwrap();
    let conn = connect_and_migrate(dir.path()).await.unwrap();

    let err = update_task_status_with_event(&conn, 999, TaskState::Done, &[])
        .await
        .unwrap_err();
    assert!(matches!(err, DbError::NotFound(_)));
}

/// The event-log foreign key refuses orphan events: reached by hitting
/// `append_event_on` directly for a task id whose row no longer exists.
#[tokio::test(flavor = "current_thread")]
async fn orphan_event_append_violates_foreign_key() {
    let dir = tempfile::tempdir().unwrap();
    let conn = connect_and_migrate(dir.path()).await.unwrap();

    // A task written without its companion event can be deleted cleanly.
    let id = insert_task(&conn, &new_task("orphan")).await.unwrap();
    conn.execute("DELETE FROM tasks WHERE id = ?1", params!(id))
        .await
        .unwrap();

    let err = append_event_on(
        &conn,
        id,
        &WorkflowEvent::TaskCompleted(TaskCompleted { id }),
    )
    .await
    .unwrap_err();
    assert!(matches!(err, DbError::Constraint(_)));
}

#[test]
fn chain_hash_genesis_and_canonicalization() {
    let raw = r#"{"z":1,"a":2}"#;
    let canonical = canonical_payload(raw).unwrap();
    assert_eq!(canonical, r#"{"a":2,"z":1}"#);

    let h1 = chain_hash(None, &canonical);
    let h2 = chain_hash(Some("GENESIS"), &canonical);
    assert_eq!(h1, h2);

    let h_next = chain_hash(Some(&h1), &canonical);
    assert_ne!(h1, h_next);
}

#[tokio::test(flavor = "current_thread")]
async fn event_chain_assigns_sequential_seq_and_hashes() {
    let dir = tempfile::tempdir().unwrap();
    let conn = connect_and_migrate(dir.path()).await.unwrap();

    let (id, _) = insert_task_with_event(&conn, &new_task("slice 1"))
        .await
        .unwrap();
    advance_subtask_with_event(&conn, id, None).await.unwrap();

    let rows = list_events_ascending(&conn).await.unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].seq, 1);
    assert_eq!(rows[1].seq, 2);

    let expected_h1 = chain_hash(None, &rows[0].canonical_payload);
    assert_eq!(rows[0].chain_hash.as_str(), expected_h1.as_str());

    let expected_h2 = chain_hash(Some(&expected_h1), &rows[1].canonical_payload);
    assert_eq!(rows[1].chain_hash.as_str(), expected_h2.as_str());
}

/// The sensor gate is re-checked inside the write transaction, so a stale
/// CLI pre-check cannot advance past a missing beat.
#[tokio::test(flavor = "current_thread")]
async fn advance_rechecks_sensor_gate_inside_transaction() {
    use crate::repo_exec::{NewBeat, record_sensor_outcome};

    let dir = tempfile::tempdir().unwrap();
    let conn = connect_and_migrate(dir.path()).await.unwrap();
    let (id, _) = insert_task_with_event(&conn, &new_task("gated"))
        .await
        .unwrap();

    let err = advance_subtask_with_event(&conn, id, Some("check"))
        .await
        .unwrap_err();
    assert!(matches!(err, DbError::GateUnsatisfied { .. }), "{err}");

    record_sensor_outcome(
        &conn,
        &NewBeat {
            task_id: Some(id),
            beat_type: "sensor",
            status: "ok",
            sensor_exit_code: Some(0),
            sensor_name: Some("check"),
            started_at: 1,
            completed_at: Some(1),
        },
        true,
        None,
    )
    .await
    .unwrap();

    advance_subtask_with_event(&conn, id, Some("check"))
        .await
        .unwrap();
}
