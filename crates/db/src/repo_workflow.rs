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

pub use do_harness_types::chain_hash;

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
    /// SHA-256 chain hash for this row (mandatory since migration 0011).
    pub chain_hash: String,
}

/// Canonicalizes a workflow event payload, mapping canonicalization failures
/// to [`DbError::InvalidEventPayload`].
///
/// # Errors
///
/// Returns an error if the payload is not valid JSON.
pub fn canonical_payload(payload_json: &str) -> Result<String> {
    do_harness_types::canonical_payload(payload_json)
        .map_err(|err| DbError::InvalidEventPayload(err.to_string()))
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
/// When `required_sensor` is set, the sensor's latest beat is re-checked
/// inside the transaction before advancing, so a concurrent writer cannot
/// slip past the CLI's pre-check.
///
/// # Errors
///
/// Returns an error when the gate fails, the update, event append, or
/// transaction fails, or when the task does not exist.
pub async fn advance_subtask_with_event(
    conn: &Connection,
    id: i64,
    required_sensor: Option<&str>,
) -> Result<(i64, WorkflowEvent)> {
    let tx = conn.transaction().await?;
    if let Some(sensor) = required_sensor {
        ensure_sensor_ok_on(&tx, id, sensor).await?;
    }
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
/// Every sensor in `required_sensors` is re-checked inside the transaction
/// (latest beat must be `ok`), closing the read-then-write gate race.
///
/// # Errors
///
/// Returns an error when the state is not terminal, a gate fails, the update,
/// event append, or transaction fails, or when the task does not exist.
pub async fn update_task_status_with_event(
    conn: &Connection,
    id: i64,
    status: TaskState,
    required_sensors: &[String],
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
    for sensor in required_sensors {
        ensure_sensor_ok_on(&tx, id, sensor).await?;
    }
    update_task_status(&tx, id, status).await?;
    append_event_on(&tx, id, &event).await?;
    tx.commit().await?;
    Ok(event)
}

/// Returns `Ok` when the task's latest beat for `sensor` is `"ok"`, otherwise
/// a [`DbError::GateUnsatisfied`]. Runs on a transaction connection so the
/// check and the dependent write commit or roll back together.
async fn ensure_sensor_ok_on(conn: &Connection, task_id: i64, sensor: &str) -> Result<()> {
    let mut rows = conn
        .query(
            "SELECT status FROM beats \
             WHERE task_id = ?1 AND beat_type = 'sensor' AND sensor_name = ?2 \
             ORDER BY id DESC LIMIT 1",
            params!(task_id, sensor),
        )
        .await?;
    match rows.next().await? {
        Some(row) if row.get::<String>(0)? == "ok" => Ok(()),
        _ => Err(DbError::GateUnsatisfied {
            task_id,
            sensor: sensor.to_owned(),
        }),
    }
}

/// Counts tasks whose `TaskAdded` event is missing from the event log.
///
/// Normal command writers always append the event in the task transaction, so
/// a non-zero count proves an out-of-band write (raw SQL or a crash in a
/// non-transactional path) and is surfaced by `doctor`.
///
/// # Errors
///
/// Returns an error when the query fails.
pub async fn count_tasks_without_added_event(conn: &Connection) -> Result<i64> {
    let mut rows = conn
        .query(
            "SELECT COUNT(*) FROM tasks t WHERE NOT EXISTS (\
             SELECT 1 FROM workflow_events e WHERE e.task_id = t.id AND e.kind = 'TaskAdded')",
            Params::None,
        )
        .await?;
    match rows.next().await? {
        Some(row) => Ok(row.get(0)?),
        None => Ok(0),
    }
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
            seq: row.get(5)?,
            chain_hash: row.get(6)?,
        });
    }
    Ok(events)
}

/// Appends one event to the log without transaction management, for composing
/// into a larger transaction.
///
/// The `seq` read-then-insert races under concurrent writers (for example a
/// pre-commit `verify --record` overlapping a manual `task advance`); the
/// `UNIQUE(seq)` constraint from migration 0011 turns the race into a
/// constraint error, and this loop re-reads the tail and retries.
async fn append_event_on(conn: &Connection, task_id: i64, event: &WorkflowEvent) -> Result<()> {
    const MAX_SEQ_RETRIES: usize = 5;

    let raw_payload = serde_json::to_string(event)
        .map_err(|err| DbError::InvalidEventPayload(err.to_string()))?;
    let payload = canonical_payload(&raw_payload)?;

    for attempt in 0..MAX_SEQ_RETRIES {
        let mut rows = conn
            .query(
                "SELECT seq, chain_hash FROM workflow_events ORDER BY seq DESC LIMIT 1",
                Params::None,
            )
            .await?;
        let (last_seq, prev_hash) = match rows.next().await? {
            Some(row) => (row.get::<i64>(0)?, Some(row.get::<String>(1)?)),
            None => (0, None),
        };
        drop(rows);
        let seq = last_seq + 1;
        let hash = chain_hash(prev_hash.as_deref(), &payload);

        match conn
            .execute(
                "INSERT INTO workflow_events (task_id, kind, payload, created_at, seq, chain_hash) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params!(
                    task_id,
                    event.name(),
                    payload.as_str(),
                    unix_now(),
                    seq,
                    hash.as_str()
                ),
            )
            .await
        {
            Ok(_) => return Ok(()),
            Err(err) => match DbError::from(err) {
                DbError::Constraint(_) if attempt + 1 < MAX_SEQ_RETRIES => {}
                other => return Err(other),
            },
        }
    }
    Err(DbError::Constraint(
        "workflow event seq retries exhausted under concurrent writers".to_owned(),
    ))
}

#[cfg(test)]
mod tests;
