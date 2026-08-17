//! Repository layer for the task table and the invariants seed.

use anyhow::{Context, Result};
use do_harness_types::{DecisionHeader, TaskRecord, TaskState};
use libsql::{Connection, params, params::Params};

use crate::migrate::unix_now;

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
pub async fn insert_task(conn: &Connection, task: &NewTask<'_>) -> Result<i64> {
    let now = unix_now();
    conn.execute(
        "INSERT INTO tasks (parent_id, title, method, subtask_index, status, \
         precondition, created_at, updated_at) \
         VALUES (?1, ?2, ?3, ?4, 'pending', ?5, ?6, ?6)",
        params!(
            task.parent_id,
            task.title,
            task.method,
            task.subtask_index,
            task.precondition,
            now
        ),
    )
    .await?;
    Ok(conn.last_insert_rowid())
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
/// # Errors
///
/// Returns an error when the update statement fails.
pub async fn update_task_status(conn: &Connection, id: i64, status: TaskState) -> Result<()> {
    conn.execute(
        "UPDATE tasks SET status = ?1, updated_at = ?2 WHERE id = ?3",
        params!(status.as_str(), unix_now(), id),
    )
    .await?;
    Ok(())
}

/// Advances a task's subtask pointer by one and marks it in progress.
///
/// Returns the new subtask index.
///
/// # Errors
///
/// Returns an error when the update or follow-up query fails.
pub async fn advance_subtask(conn: &Connection, id: i64) -> Result<i64> {
    conn.execute(
        "UPDATE tasks SET subtask_index = subtask_index + 1, status = 'in_progress', \
         updated_at = ?1 WHERE id = ?2",
        params!(unix_now(), id),
    )
    .await?;
    let mut rows = conn
        .query("SELECT subtask_index FROM tasks WHERE id = ?1", params!(id))
        .await?;
    let row = rows
        .next()
        .await?
        .context("task vanished after advancing subtask")?;
    Ok(row.get(0)?)
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
            .map_err(|err| anyhow::anyhow!("invalid task status '{status}': {err}"))?,
        precondition: row.get(6)?,
        created_at: row.get(7)?,
        updated_at: row.get(8)?,
    })
}

/// Upserts a collection of decision headers into the `invariants` table.
///
/// Existing invariants are matched on their `invariant` text and updated;
/// new ones are inserted. Returns the number of invariants written.
///
/// # Errors
///
/// Returns an error if any upsert statement fails.
pub async fn seed_invariants(conn: &Connection, headers: &[DecisionHeader]) -> Result<usize> {
    let now = unix_now();
    let mut written = 0;
    for header in headers {
        conn.execute(
            "INSERT INTO invariants (invariant, rationale, sensor, category, created_at) \
             VALUES (?1, ?2, ?3, ?4, ?5) \
             ON CONFLICT(invariant) DO UPDATE SET \
               rationale = excluded.rationale, \
               sensor = excluded.sensor, \
               category = excluded.category",
            params!(
                header.invariant.as_str(),
                header.rationale.as_str(),
                header.sensor.as_str(),
                header.category.as_str(),
                now
            ),
        )
        .await?;
        written += 1;
    }
    Ok(written)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

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
        seed_invariants(&conn, &first).await.unwrap();
        let second = vec![DecisionHeader::new(
            "inv".into(),
            "r2".into(),
            "s2".into(),
            "contracts".into(),
        )];
        seed_invariants(&conn, &second).await.unwrap();

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
