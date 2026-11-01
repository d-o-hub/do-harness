#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;
use crate::migrate::connect_and_migrate;
use do_harness_types::{BeatStatus, BeatType, TaskCompleted};

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
/// `append_event_on` directly for a task id whose row no longer exists. Pins
/// the classification as well: only a lost `UNIQUE(seq)` race is retryable, so
/// a foreign-key failure must not be mistaken for one.
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
    assert!(matches!(err, DbError::Constraint { code: 787, .. }));
    assert!(!err.is_unique_violation());
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
            scope: None,
            beat_type: BeatType::Sensor,
            status: BeatStatus::Ok,
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

/// The writer derives the event from the command, and the command's extra
/// fields stay in the row rather than in the published payload.
///
/// `TaskAdded` is a released type: adding `parent_id`/`precondition` to it broke
/// every downstream exhaustive literal (`constructible_struct_adds_field`), so
/// the payload keeps its three fields while the row keeps the rest. This test
/// pins both halves — an event built by hand in the writer would drop one of
/// them, and a payload that grew would be an unreleased API change.
#[tokio::test(flavor = "current_thread")]
async fn task_added_event_is_derived_from_the_command() {
    let dir = tempfile::tempdir().unwrap();
    let conn = connect_and_migrate(dir.path()).await.unwrap();

    let (parent_id, _) = insert_task_with_event(&conn, &new_task("parent"))
        .await
        .unwrap();
    let child = NewTask {
        title: "child",
        method: Some("mini"),
        subtask_index: 0,
        precondition: Some("parent guard holds"),
        parent_id: Some(parent_id),
    };
    let (child_id, event) = insert_task_with_event(&conn, &child).await.unwrap();

    let WorkflowEvent::TaskAdded(added) = event else {
        panic!("expected TaskAdded, got {event:?}");
    };
    assert_eq!(
        added,
        AddTask {
            title: "child".to_owned(),
            method: Some("mini".to_owned()),
            parent_id: Some(parent_id),
            precondition: Some("parent guard holds".to_owned()),
        }
        .event(child_id),
        "the appended event must be the command's own event, not a hand-built one"
    );

    // The fields the payload does not carry still reach the row.
    let record = crate::repo::get_task(&conn, child_id)
        .await
        .unwrap()
        .expect("child row");
    assert_eq!(record.parent_id, Some(parent_id));
    assert_eq!(record.precondition.as_deref(), Some("parent guard holds"));

    let stored = list_events_ascending(&conn).await.unwrap();
    let appended = stored
        .iter()
        .find(|row| row.task_id == child_id)
        .expect("child TaskAdded event is stored");
    assert!(
        !appended.canonical_payload.contains("parent guard holds"),
        "the published payload must not carry a field the released type lacks: {}",
        appended.canonical_payload
    );
}
