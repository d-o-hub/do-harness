//! Append-only workflow event log and transactional task command writers.
//!
//! Each command function mutates the `tasks` row and persists the emitted
//! [`WorkflowEvent`] in a single transaction, so the event stream always
//! covers every persisted task transition. Read models fold this stream
//! (see `list_all_events`) instead of reconstructing events from row state.

use crate::error::{DbError, Result};
use crate::migrate::unix_now;
use crate::repo::{NewTask, advance_subtask, insert_task, update_task_status};
use do_harness_types::{
    DomainEvent, TaskAdded, TaskAdvanced, TaskCompleted, TaskFailed, TaskState, WorkflowEvent,
};
use libsql::{Connection, params, params::Params};
use sha2::{Digest, Sha256};

/// Structured database row from `workflow_events` with sequence and hash chain.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkflowEventRow {
    /// Row primary key.
    pub id: i64,
    /// Foreign key to task.
    pub task_id: i64,
    /// Event kind.
    pub kind: String,
    /// Raw stored payload.
    pub payload: String,
    /// Canonicalized payload string.
    pub canonical_payload: String,
    /// Creation timestamp.
    pub created_at: i64,
    /// Monotonic sequence number in the chain.
    pub seq: i64,
    /// SHA-256 chain hash for this row.
    pub chain_hash: Option<String>,
}

/// Canonicalizes a JSON string by parsing into `serde_json::Value` and re-serializing,
/// ensuring object keys are sorted deterministically.
///
/// # Errors
///
/// Returns an error if the payload is not valid JSON or re-serialization fails.
pub fn canonical_payload(payload_json: &str) -> Result<String> {
    let value: serde_json::Value = serde_json::from_str(payload_json)
        .map_err(|err| DbError::InvalidEventPayload(err.to_string()))?;
    serde_json::to_string(&value).map_err(|err| DbError::InvalidEventPayload(err.to_string()))
}

/// Computes SHA-256 chain hash: `SHA-256(prev || "|" || payload_json)`.
///
/// If `prev` is [`None`], defaults to `"GENESIS"`.
#[must_use]
pub fn chain_hash(prev: Option<&str>, payload_json: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(prev.unwrap_or("GENESIS").as_bytes());
    hasher.update(b"|");
    hasher.update(payload_json.as_bytes());
    hex::encode(hasher.finalize())
}

/// Inserts a task in `pending` state and persists its `TaskAdded` event in
/// one transaction. Returns the new id and the persisted event.
///
/// # Errors
///
/// Returns an error when the insert, event append, or transaction fails.
pub async fn insert_task_with_event(
    conn: &Connection,
    task: &NewTask<'_>,
) -> Result<(i64, WorkflowEvent)> {
    let tx = conn.transaction().await?;
    let id = insert_task(&tx, task).await?;
    let event = WorkflowEvent::TaskAdded(TaskAdded {
        id,
        title: task.title.to_owned(),
        method: task.method.map(ToOwned::to_owned),
    });
    append_event_on(&tx, id, &event).await?;
    tx.commit().await?;
    Ok((id, event))
}

/// Advances a task's subtask pointer and persists its `TaskAdvanced` event in
/// one transaction. Returns the new subtask index and the persisted event.
///
/// # Errors
///
/// Returns an error when the update, event append, or transaction fails, or
/// when the task does not exist.
pub async fn advance_subtask_with_event(
    conn: &Connection,
    id: i64,
) -> Result<(i64, WorkflowEvent)> {
    let tx = conn.transaction().await?;
    let index = advance_subtask(&tx, id).await?;
    let event = WorkflowEvent::TaskAdvanced(TaskAdvanced {
        id,
        subtask_index: index,
    });
    append_event_on(&tx, id, &event).await?;
    tx.commit().await?;
    Ok((index, event))
}

/// Updates a task to a terminal state (`done` or `failed`) and persists the
/// matching `TaskCompleted`/`TaskFailed` event in one transaction.
/// Non-terminal states are rejected: lifecycle entry points own them.
///
/// # Errors
///
/// Returns an error when the state is not terminal, when the update, event
/// append, or transaction fails, or when the task does not exist.
pub async fn update_task_status_with_event(
    conn: &Connection,
    id: i64,
    status: TaskState,
) -> Result<WorkflowEvent> {
    let event = match status {
        TaskState::Done => WorkflowEvent::TaskCompleted(TaskCompleted { id }),
        TaskState::Failed => WorkflowEvent::TaskFailed(TaskFailed { id }),
        other => {
            return Err(DbError::InvalidTerminalState(other.as_str().to_owned()));
        }
    };
    let tx = conn.transaction().await?;
    if crate::repo::get_task(&tx, id).await?.is_none() {
        return Err(DbError::NotFound(format!("task {id} not found")));
    }
    update_task_status(&tx, id, status).await?;
    append_event_on(&tx, id, &event).await?;
    tx.commit().await?;
    Ok(event)
}

/// Loads the full workflow event stream as `(task_id, event)` pairs ordered
/// by persistence order.
///
/// # Errors
///
/// Returns an error when the query fails or a stored payload cannot be
/// deserialized.
pub async fn list_all_events(conn: &Connection) -> Result<Vec<(i64, WorkflowEvent)>> {
    let mut rows = conn
        .query(
            "SELECT task_id, payload FROM workflow_events ORDER BY id",
            Params::None,
        )
        .await?;
    let mut events = Vec::new();
    while let Some(row) = rows.next().await? {
        let task_id: i64 = row.get(0)?;
        let payload: String = row.get(1)?;
        let event = serde_json::from_str::<WorkflowEvent>(&payload)
            .map_err(|err| DbError::InvalidEventPayload(format!("{err} (payload: {payload})")))?;
        events.push((task_id, event));
    }
    Ok(events)
}

/// Loads all workflow event rows in ascending insertion order with chain metadata.
///
/// # Errors
///
/// Returns an error when the database query fails or a payload cannot be canonicalized.
pub async fn list_events_ascending(conn: &Connection) -> Result<Vec<WorkflowEventRow>> {
    let mut rows = conn
        .query(
            "SELECT id, task_id, kind, payload, created_at, seq, chain_hash \
             FROM workflow_events ORDER BY id ASC",
            Params::None,
        )
        .await?;
    let mut events = Vec::new();
    while let Some(row) = rows.next().await? {
        let payload: String = row.get(3)?;
        let canonical = canonical_payload(&payload)?;
        events.push(WorkflowEventRow {
            id: row.get(0)?,
            task_id: row.get(1)?,
            kind: row.get(2)?,
            payload: payload.clone(),
            canonical_payload: canonical,
            created_at: row.get(4)?,
            seq: row.get::<i64>(5).unwrap_or(0),
            chain_hash: row.get(6)?,
        });
    }
    Ok(events)
}

/// Appends one event to the log without transaction management, for composing
/// into a larger transaction.
async fn append_event_on(conn: &Connection, task_id: i64, event: &WorkflowEvent) -> Result<()> {
    let raw_payload = serde_json::to_string(event)
        .map_err(|err| DbError::InvalidEventPayload(err.to_string()))?;
    let payload = canonical_payload(&raw_payload)?;

    let mut rows = conn
        .query(
            "SELECT seq, chain_hash FROM workflow_events WHERE seq IS NOT NULL ORDER BY seq DESC LIMIT 1",
            Params::None,
        )
        .await?;
    let (last_seq, prev_hash) = match rows.next().await? {
        Some(row) => {
            let s: i64 = row.get(0)?;
            let h: Option<String> = row.get(1)?;
            (s, h)
        }
        None => (0, None),
    };
    let seq = last_seq + 1;
    let hash = chain_hash(prev_hash.as_deref(), &payload);

    conn.execute(
        "INSERT INTO workflow_events (task_id, kind, payload, created_at, seq, chain_hash) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params!(
            task_id,
            event.name(),
            payload,
            unix_now(),
            seq,
            hash.as_str()
        ),
    )
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
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
        let (index, _) = advance_subtask_with_event(&conn, id).await.unwrap();
        assert_eq!(index, 1);
        update_task_status_with_event(&conn, id, TaskState::Done)
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
            let err = update_task_status_with_event(&conn, id, state)
                .await
                .unwrap_err();
            assert!(matches!(err, DbError::InvalidTerminalState(_)));
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn advance_subtask_reports_missing_task_as_not_found() {
        let dir = tempfile::tempdir().unwrap();
        let conn = connect_and_migrate(dir.path()).await.unwrap();

        let err = advance_subtask_with_event(&conn, 999).await.unwrap_err();
        assert!(matches!(err, DbError::NotFound(_)));
    }

    /// The terminal writer checks task existence inside its transaction, so a
    /// missing id surfaces as `NotFound` instead of a raw constraint error.
    #[tokio::test(flavor = "current_thread")]
    async fn terminal_writer_reports_missing_task_as_not_found() {
        let dir = tempfile::tempdir().unwrap();
        let conn = connect_and_migrate(dir.path()).await.unwrap();

        let err = update_task_status_with_event(&conn, 999, TaskState::Done)
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
        advance_subtask_with_event(&conn, id).await.unwrap();

        let rows = list_events_ascending(&conn).await.unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].seq, 1);
        assert_eq!(rows[1].seq, 2);

        let expected_h1 = chain_hash(None, &rows[0].canonical_payload);
        assert_eq!(rows[0].chain_hash.as_deref(), Some(expected_h1.as_str()));

        let expected_h2 = chain_hash(Some(&expected_h1), &rows[1].canonical_payload);
        assert_eq!(rows[1].chain_hash.as_deref(), Some(expected_h2.as_str()));
    }
}
