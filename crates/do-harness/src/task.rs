//! Task state queries and exports for `do-harness task`.

use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use do_harness_types::TaskRecord;
use serde::Serialize;

use crate::report::Format;

/// Snapshot of the task list written to `plans/tasks.json`.
#[derive(Debug, Clone, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TaskSnapshot {
    /// Unix timestamp of the export.
    pub exported_at: i64,
    /// All tasks ordered by id.
    pub tasks: Vec<TaskRecord>,
}

/// Writes `plans/tasks.json` with the full task list; returns the task count.
///
/// The libSQL store stays the source of truth; the file is an
/// agent-readable snapshot.
///
/// # Errors
///
/// Returns an error when the state database cannot be opened or the snapshot
/// cannot be written.
pub async fn export_tasks(root: &Path) -> Result<usize> {
    let conn = do_harness_db::connect_and_migrate(root).await?;
    let tasks = do_harness_db::list_tasks(&conn).await?;
    let snapshot = TaskSnapshot {
        exported_at: do_harness_db::unix_now(),
        tasks,
    };
    let json =
        serde_json::to_string_pretty(&snapshot).context("failed to serialize task snapshot")?;
    let path = root.join("plans/tasks.json");
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    fs::write(&path, format!("{json}\n"))
        .with_context(|| format!("failed to write {}", path.display()))?;
    Ok(snapshot.tasks.len())
}

/// Prints the task list in the requested format.
///
/// # Errors
///
/// Returns an error when the state database cannot be opened.
pub async fn list_tasks(root: &Path, format: Format) -> Result<()> {
    let conn = do_harness_db::connect_and_migrate(root).await?;
    let tasks = do_harness_db::list_tasks(&conn).await?;
    match format {
        Format::Text => {
            for task in &tasks {
                println!(
                    "{}: {} [{}] subtask_index={}",
                    task.id,
                    task.title,
                    task.status.as_str(),
                    task.subtask_index
                );
            }
        }
        Format::Json => {
            println!(
                "{}",
                serde_json::to_string(&tasks).context("failed to serialize tasks")?
            );
        }
    }
    Ok(())
}

/// Inserts a new task in `pending` state with `subtask_index = 0`.
///
/// The parent link is persisted when `parent_id` is given, keeping the
/// hierarchical task network intact for later workflow runs. Returns the new
/// task id.
///
/// # Errors
///
/// Returns an error when the state database cannot be opened or the insert
/// fails.
pub async fn add_task(
    root: &Path,
    title: &str,
    method: Option<&str>,
    parent_id: Option<i64>,
    precondition: Option<&str>,
) -> Result<i64> {
    let conn = do_harness_db::connect_and_migrate(root).await?;
    let id = do_harness_db::insert_task(
        &conn,
        &do_harness_db::NewTask {
            title,
            method,
            subtask_index: 0,
            precondition,
            parent_id,
        },
    )
    .await?;
    Ok(id)
}

/// Advances the subtask pointer of a task and returns the new index.
///
/// The task must exist; `advance_subtask` also sets the status to
/// `in_progress`.
///
/// # Errors
///
/// Returns an error when the state database cannot be opened, when no task
/// with the given id exists, or when the advance fails.
pub async fn advance_task(root: &Path, id: i64) -> Result<i64> {
    let conn = do_harness_db::connect_and_migrate(root).await?;
    if do_harness_db::get_task(&conn, id).await?.is_none() {
        anyhow::bail!("task {id} not found");
    }
    let index = do_harness_db::advance_subtask(&conn, id).await?;
    Ok(index)
}

/// Marks a task as failed.
///
/// The task must exist.
///
/// # Errors
///
/// Returns an error when the state database cannot be opened, when no task
/// with the given id exists, or when the status update fails.
pub async fn fail_task(root: &Path, id: i64) -> Result<()> {
    let conn = do_harness_db::connect_and_migrate(root).await?;
    if do_harness_db::get_task(&conn, id).await?.is_none() {
        anyhow::bail!("task {id} not found");
    }
    do_harness_db::update_task_status(&conn, id, do_harness_types::TaskState::Failed).await
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

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
}
