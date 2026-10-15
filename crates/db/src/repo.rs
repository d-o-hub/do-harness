//! Repository layer for the task table and the invariants seed.

use crate::error::{DbError, Result};
use crate::migrate::unix_now;
use do_harness_types::{DecisionHeader, TaskRecord, TaskState};
use libsql::{Connection, params, params::Params};

/// Insert parameters for a new task.
#[derive(Debug, Clone)]
pub struct NewTask<'a> {
    /// Human-readable task title.
    pub title: &'a str,
    /// Name of the HTN method this task follows.
    pub method: Option<&'a str>,
    /// Index of the current subtask within the method.
    pub subtask_index: i64,
    /// Recorded precondition guard.
    pub precondition: Option<&'a str>,
    /// Parent task id, when this task is a subtask.
    pub parent_id: Option<i64>,
}

const TASK_COLUMNS: &str = "id, parent_id, title, method, subtask_index, status, \
                            precondition, created_at, updated_at";

/// Inserts a task in `pending` state and returns its id.
///
/// # Errors
///
/// Returns an error when the insert statement fails.
pub(crate) async fn insert_task(conn: &Connection, task: &NewTask<'_>) -> Result<i64> {
    let now = unix_now();
    let mut rows = conn
        .query(
            "INSERT INTO tasks (parent_id, title, method, subtask_index, status, \
             precondition, created_at, updated_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7) \
             RETURNING id",
            params!(
                task.parent_id,
                task.title,
                task.method,
                task.subtask_index,
                TaskState::Pending.as_str(),
                task.precondition,
                now
            ),
        )
        .await?;
    let row = rows
        .next()
        .await?
        .ok_or_else(|| DbError::NotFound("task id vanished after insert".to_string()))?;
    Ok(row.get(0)?)
}

/// Fetches a task by id.
///
/// # Errors
///
/// Returns an error when the query fails.
pub async fn get_task(conn: &Connection, id: i64) -> Result<Option<TaskRecord>> {
    let mut rows = conn
        .query(
            &format!("SELECT {TASK_COLUMNS} FROM tasks WHERE id = ?1"),
            params!(id),
        )
        .await?;
    match rows.next().await? {
        Some(row) => Ok(Some(task_from_row(&row)?)),
        None => Ok(None),
    }
}

/// Lists all tasks in insertion order.
///
/// # Errors
///
/// Returns an error when the query fails.
pub async fn list_tasks(conn: &Connection) -> Result<Vec<TaskRecord>> {
    let mut rows = conn
        .query(
            &format!("SELECT {TASK_COLUMNS} FROM tasks ORDER BY id"),
            Params::None,
        )
        .await?;
    let mut tasks = Vec::new();
    while let Some(row) = rows.next().await? {
        tasks.push(task_from_row(&row)?);
    }
    Ok(tasks)
}

/// Updates a task's lifecycle state.
///
/// The update reports the row back via `RETURNING`, so a missing task id is
/// reported directly instead of silently updating zero rows. The `WHERE` also
/// excludes tasks already in a terminal state, so a second completion (or a
/// completion of a `failed` task) fails closed instead of appending a
/// duplicate terminal event.
///
/// # Errors
///
/// Returns an error when the update statement fails, the task does not exist,
/// or the task is already terminal.
pub(crate) async fn update_task_status(
    conn: &Connection,
    id: i64,
    status: TaskState,
) -> Result<()> {
    let mut rows = conn
        .query(
            "UPDATE tasks SET status = ?1, updated_at = ?2 WHERE id = ?3 \
             AND status NOT IN ('done', 'failed') RETURNING id",
            params!(status.as_str(), unix_now(), id),
        )
        .await?;
    if rows.next().await?.is_none() {
        return Err(missing_row_error(conn, id, "update the status of").await);
    }
    Ok(())
}

/// Advances a task's subtask pointer by one and marks it in progress.
///
/// Returns the new subtask index. The update returns the new index via
/// `RETURNING`, so a missing task id is reported directly instead of failing
/// on a follow-up query. Terminal tasks are excluded: advancing a `done` or
/// `failed` task would append a `TaskAdvanced` after its terminal event.
///
/// # Errors
///
/// Returns an error when the update fails, the task does not exist, or the
/// task is already terminal.
pub(crate) async fn advance_subtask(conn: &Connection, id: i64) -> Result<i64> {
    let mut rows = conn
        .query(
            "UPDATE tasks SET subtask_index = subtask_index + 1, status = ?1, \
             updated_at = ?2 WHERE id = ?3 AND status NOT IN ('done', 'failed') \
             RETURNING subtask_index",
            params!(TaskState::InProgress.as_str(), unix_now(), id),
        )
        .await?;
    match rows.next().await? {
        Some(row) => Ok(row.get(0)?),
        None => Err(missing_row_error(conn, id, "advance").await),
    }
}

/// Classifies a guarded `UPDATE ... RETURNING` that matched no row: a missing
/// task is [`DbError::NotFound`], an existing terminal task is
/// [`DbError::InvalidTaskTransition`].
async fn missing_row_error(conn: &Connection, id: i64, action: &str) -> DbError {
    match get_task(conn, id).await {
        Ok(Some(task)) => DbError::InvalidTaskTransition(format!(
            "cannot {action} task {id}: it is already '{}' (terminal)",
            task.status.as_str()
        )),
        _ => DbError::NotFound(format!("task {id} not found")),
    }
}

/// Maps a `tasks` row to a [`TaskRecord`].
fn task_from_row(row: &libsql::Row) -> Result<TaskRecord> {
    let status: String = row.get(5)?;
    Ok(TaskRecord {
        id: row.get(0)?,
        parent_id: row.get(1)?,
        title: row.get(2)?,
        method: row.get(3)?,
        subtask_index: row.get(4)?,
        status: TaskState::try_from(status.as_str())
            .map_err(|err| DbError::InvalidTaskStatus(format!("{status}: {err}")))?,
        precondition: row.get(6)?,
        created_at: row.get(7)?,
        updated_at: row.get(8)?,
    })
}

/// Upserts decision headers into the `invariants` table.
///
/// When `prune` is set, invariants absent from `headers` are deleted so the
/// table mirrors `plans/invariants.json` exactly (stale rows otherwise persist
/// forever because the upsert only inserts/updates). Returns the number of
/// headers written.
///
/// # Errors
///
/// Returns an error when the upsert, prune, or transaction fails.
pub async fn seed_invariants(
    conn: &Connection,
    headers: &[DecisionHeader],
    prune: bool,
) -> Result<usize> {
    let now = unix_now();
    let tx = crate::tx::begin_immediate(conn).await?;
    for header in headers {
        tx.execute(
            "INSERT INTO invariants (invariant, rationale, sensor, category, created_at, updated_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6) \
             ON CONFLICT(invariant) DO UPDATE SET \
               rationale = excluded.rationale, \
               sensor = excluded.sensor, \
               category = excluded.category, \
               updated_at = excluded.updated_at",
            params!(
                header.invariant.as_str(),
                header.rationale.as_str(),
                header.sensor.as_str(),
                header.category.as_str(),
                now,
                now
            ),
        )
        .await?;
    }
    if prune {
        let wanted: std::collections::HashSet<&str> = headers
            .iter()
            .map(|header| header.invariant.as_str())
            .collect();
        let mut rows = tx
            .query("SELECT invariant FROM invariants", Params::None)
            .await?;
        let mut stale: Vec<String> = Vec::new();
        while let Some(row) = rows.next().await? {
            let name: String = row.get(0)?;
            if !wanted.contains(name.as_str()) {
                stale.push(name);
            }
        }
        drop(rows);
        for name in stale {
            tx.execute("DELETE FROM invariants WHERE invariant = ?1", params!(name))
                .await?;
        }
    }
    tx.commit().await?;
    Ok(headers.len())
}

/// Highest `tasks.updated_at`, used to detect stale `plans/tasks.json` exports.
///
/// # Errors
///
/// Returns an error when the query fails.
pub async fn latest_task_update(conn: &Connection) -> Result<Option<i64>> {
    let mut rows = conn
        .query("SELECT MAX(updated_at) FROM tasks", Params::None)
        .await?;
    match rows.next().await? {
        Some(row) => Ok(row.get(0)?),
        None => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    fn new_task(title: &str) -> NewTask<'_> {
        NewTask {
            title,
            method: Some("vertical-event-slice"),
            subtask_index: 0,
            precondition: None,
            parent_id: None,
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn insert_task_roundtrips_via_get() {
        let dir = tempfile::tempdir().unwrap();
        let conn = crate::migrate::connect_and_migrate(dir.path())
            .await
            .unwrap();

        let id = insert_task(&conn, &new_task("slice")).await.unwrap();
        let task = get_task(&conn, id).await.unwrap().unwrap();

        assert_eq!(task.title, "slice");
        assert_eq!(task.method.as_deref(), Some("vertical-event-slice"));
        assert_eq!(task.status, TaskState::Pending);
        assert_eq!(task.subtask_index, 0);
        assert!(get_task(&conn, id + 1).await.unwrap().is_none());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn list_tasks_returns_insertion_order() {
        let dir = tempfile::tempdir().unwrap();
        let conn = crate::migrate::connect_and_migrate(dir.path())
            .await
            .unwrap();
        insert_task(&conn, &new_task("first")).await.unwrap();
        insert_task(&conn, &new_task("second")).await.unwrap();

        let tasks = list_tasks(&conn).await.unwrap();

        assert_eq!(tasks.len(), 2);
        assert_eq!(tasks[0].title, "first");
        assert_eq!(tasks[1].title, "second");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn update_task_status_changes_state() {
        let dir = tempfile::tempdir().unwrap();
        let conn = crate::migrate::connect_and_migrate(dir.path())
            .await
            .unwrap();
        let id = insert_task(&conn, &new_task("slice")).await.unwrap();

        update_task_status(&conn, id, TaskState::Failed)
            .await
            .unwrap();

        assert_eq!(
            get_task(&conn, id).await.unwrap().unwrap().status,
            TaskState::Failed
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn update_task_status_reports_missing_task_as_not_found() {
        let dir = tempfile::tempdir().unwrap();
        let conn = crate::migrate::connect_and_migrate(dir.path())
            .await
            .unwrap();

        let err = update_task_status(&conn, 999, TaskState::Failed)
            .await
            .unwrap_err();
        assert!(matches!(err, crate::error::DbError::NotFound(_)));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn advance_subtask_increments_and_marks_in_progress() {
        let dir = tempfile::tempdir().unwrap();
        let conn = crate::migrate::connect_and_migrate(dir.path())
            .await
            .unwrap();
        let id = insert_task(&conn, &new_task("slice")).await.unwrap();

        let index = advance_subtask(&conn, id).await.unwrap();

        assert_eq!(index, 1);
        let task = get_task(&conn, id).await.unwrap().unwrap();
        assert_eq!(task.subtask_index, 1);
        assert_eq!(task.status, TaskState::InProgress);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn advance_subtask_rejects_terminal_task() {
        let dir = tempfile::tempdir().unwrap();
        let conn = crate::migrate::connect_and_migrate(dir.path())
            .await
            .unwrap();
        let id = insert_task(&conn, &new_task("slice")).await.unwrap();
        update_task_status(&conn, id, TaskState::Done)
            .await
            .unwrap();

        let err = advance_subtask(&conn, id).await.unwrap_err();
        assert!(
            matches!(err, crate::error::DbError::InvalidTaskTransition(_)),
            "{err:?}"
        );
        let task = get_task(&conn, id).await.unwrap().unwrap();
        assert_eq!(task.status, TaskState::Done);
        assert_eq!(task.subtask_index, 0, "terminal row must be untouched");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn update_task_status_rejects_second_terminal_transition() {
        let dir = tempfile::tempdir().unwrap();
        let conn = crate::migrate::connect_and_migrate(dir.path())
            .await
            .unwrap();
        let id = insert_task(&conn, &new_task("slice")).await.unwrap();
        update_task_status(&conn, id, TaskState::Done)
            .await
            .unwrap();

        let err = update_task_status(&conn, id, TaskState::Failed)
            .await
            .unwrap_err();
        assert!(
            matches!(err, crate::error::DbError::InvalidTaskTransition(_)),
            "{err:?}"
        );
        assert_eq!(
            get_task(&conn, id).await.unwrap().unwrap().status,
            TaskState::Done
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn seed_invariants_upserts_on_invariant_text() {
        let dir = tempfile::tempdir().unwrap();
        let conn = crate::migrate::connect_and_migrate(dir.path())
            .await
            .unwrap();
        let first = vec![DecisionHeader::new(
            "inv".into(),
            "r1".into(),
            "s1".into(),
            "contracts".into(),
        )];
        seed_invariants(&conn, &first, false).await.unwrap();
        let second = vec![DecisionHeader::new(
            "inv".into(),
            "r2".into(),
            "s2".into(),
            "contracts".into(),
        )];
        seed_invariants(&conn, &second, false).await.unwrap();

        let mut rows = conn
            .query("SELECT COUNT(*) FROM invariants", Params::None)
            .await
            .unwrap();
        let n: i64 = rows.next().await.unwrap().unwrap().get(0).unwrap();
        assert_eq!(n, 1);

        let mut rows = conn
            .query(
                "SELECT rationale FROM invariants WHERE invariant = 'inv'",
                Params::None,
            )
            .await
            .unwrap();
        let rationale: String = rows.next().await.unwrap().unwrap().get(0).unwrap();
        assert_eq!(rationale, "r2");
    }
}
