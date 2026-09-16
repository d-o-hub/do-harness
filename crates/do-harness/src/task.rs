//! Task state queries and exports for `do-harness task`.

use std::fmt::Write;
use std::path::Path;

use anyhow::{Context, Result};
use do_harness_types::{Projection, TaskBoard, TaskRecord};
use serde::{Deserialize, Serialize};

use crate::report::Format;

/// Snapshot of the task list written to `plans/tasks.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskSnapshot {
    /// Unix timestamp of the export.
    pub exported_at: i64,
    /// All tasks ordered by id.
    pub tasks: Vec<TaskRecord>,
    /// Read-model board summary at export time.
    pub summary: TaskSummary,
}

/// Board summary embedded in the export snapshot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskSummary {
    /// Tasks in `pending`.
    pub pending: i64,
    /// Tasks in `in_progress`.
    pub in_progress: i64,
    /// Tasks in `done`.
    pub done: i64,
    /// Tasks in `failed`.
    pub failed: i64,
}

/// Writes `plans/tasks.json` or custom output with the task list; returns task count.
pub async fn export_tasks(
    root: &Path,
    output: Option<&Path>,
    stdout: bool,
    format: Format,
) -> Result<usize> {
    let conn = do_harness_db::connect_and_migrate(root).await?;
    // Snapshot the board inside one read transaction so a concurrent task
    // command cannot produce a half-updated export.
    let tx = conn.transaction().await?;
    let tasks = do_harness_db::list_tasks(&tx).await?;
    let mut board = TaskBoard::new();
    for (_, event) in do_harness_db::list_all_events(&tx).await? {
        board
            .apply(&event)
            .context("persisted workflow event is not part of the workflow stream")?;
    }
    tx.commit().await?;
    let snapshot = TaskSnapshot {
        exported_at: do_harness_db::unix_now(),
        tasks,
        summary: TaskSummary {
            pending: board.pending(),
            in_progress: board.in_progress(),
            done: board.done(),
            failed: board.failed(),
        },
    };
    if stdout {
        match format {
            Format::Json => println!("{}", serde_json::to_string_pretty(&snapshot)?),
            Format::Text => {
                for t in &snapshot.tasks {
                    println!("{}: {} [{}]", t.id, t.title, t.status.as_str());
                }
            }
        }
        return Ok(snapshot.tasks.len());
    }

    let target_path = output.map_or_else(|| root.join("plans/tasks.json"), Path::to_path_buf);
    let content = match format {
        Format::Json => format!(
            "{}\n",
            serde_json::to_string_pretty(&snapshot).context("failed to serialize task snapshot")?
        ),
        Format::Text => {
            let mut s = String::new();
            for t in &snapshot.tasks {
                let _ = writeln!(s, "{}: {} [{}]", t.id, t.title, t.status.as_str());
            }
            s
        }
    };
    if let Some(parent) = target_path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    // Write to a sibling temp file and rename: readers never observe a
    // truncated snapshot even if the process dies mid-write.
    let tmp_path = target_path.with_extension("tmp");
    tokio::fs::write(&tmp_path, content)
        .await
        .with_context(|| format!("failed to write {}", tmp_path.display()))?;
    tokio::fs::rename(&tmp_path, &target_path)
        .await
        .with_context(|| format!("failed to replace {}", target_path.display()))?;
    Ok(snapshot.tasks.len())
}

/// Validates a `plans/tasks.json` snapshot against the state database.
///
/// With `check`, any drift fails the command; without it the drift is reported
/// but the command succeeds (a dry-run view of what a future import would need
/// to reconcile). Writing snapshots back into the database is intentionally
/// not supported: task ids are database-assigned and the workflow event log is
/// append-only, so imports must go through `task add`.
///
/// # Errors
///
/// Returns an error when the snapshot cannot be read or parsed, or when
/// `check` is set and the snapshot drifts from the database.
pub async fn import_tasks(root: &Path, file: Option<&Path>, check: bool) -> Result<()> {
    let path = file.map_or_else(|| root.join("plans/tasks.json"), Path::to_path_buf);
    let raw = tokio::fs::read_to_string(&path)
        .await
        .with_context(|| format!("failed to read {}", path.display()))?;
    let snapshot: TaskSnapshot = serde_json::from_str(&raw)
        .with_context(|| format!("invalid task snapshot at {}", path.display()))?;

    let conn = do_harness_db::connect_and_migrate(root).await?;
    let db_tasks = do_harness_db::list_tasks(&conn).await?;

    let mut diffs = Vec::new();
    for task in &snapshot.tasks {
        match db_tasks.iter().find(|db| db.id == task.id) {
            None => diffs.push(format!(
                "task {} '{}' present in snapshot, missing from database",
                task.id, task.title
            )),
            Some(db) => {
                if db.status != task.status {
                    diffs.push(format!(
                        "task {} status: snapshot={} database={}",
                        task.id,
                        task.status.as_str(),
                        db.status.as_str()
                    ));
                }
                if db.subtask_index != task.subtask_index {
                    diffs.push(format!(
                        "task {} subtask_index: snapshot={} database={}",
                        task.id, task.subtask_index, db.subtask_index
                    ));
                }
                if db.title != task.title {
                    diffs.push(format!("task {} title differs from database", task.id));
                }
            }
        }
    }
    for db in &db_tasks {
        if !snapshot.tasks.iter().any(|task| task.id == db.id) {
            diffs.push(format!(
                "task {} '{}' present in database, missing from snapshot",
                db.id, db.title
            ));
        }
    }

    println!(
        "snapshot: {} task(s) exported_at={}; database: {} task(s); {} difference(s)",
        snapshot.tasks.len(),
        snapshot.exported_at,
        db_tasks.len(),
        diffs.len()
    );
    for diff in &diffs {
        println!("  {diff}");
    }
    if check && !diffs.is_empty() {
        anyhow::bail!("task snapshot drift: {} difference(s)", diffs.len());
    }
    Ok(())
}

/// Prints the task list in the requested format with optional filtering.
pub async fn list_tasks(
    root: &Path,
    format: Format,
    status_filter: Option<&str>,
    method_filter: Option<&str>,
    parent_filter: Option<i64>,
) -> Result<()> {
    let conn = do_harness_db::connect_and_migrate(root).await?;
    let all_tasks = do_harness_db::list_tasks(&conn).await?;

    let tasks: Vec<TaskRecord> = all_tasks
        .into_iter()
        .filter(|t| {
            if let Some(st) = status_filter {
                if t.status.as_str() != st {
                    return false;
                }
            }
            if let Some(m) = method_filter {
                if t.method.as_deref() != Some(m) {
                    return false;
                }
            }
            if let Some(p) = parent_filter {
                if t.parent_id != Some(p) {
                    return false;
                }
            }
            true
        })
        .collect();

    let mut board = TaskBoard::new();
    for (_, event) in do_harness_db::list_all_events(&conn).await? {
        board
            .apply(&event)
            .context("persisted workflow event is not part of the workflow stream")?;
    }

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
            println!(
                "summary: pending={} in_progress={} done={} failed={}",
                board.pending(),
                board.in_progress(),
                board.done(),
                board.failed()
            );
        }
        Format::Json => {
            let json = serde_json::json!({
                "tasks": tasks,
                "summary": {
                    "pending": board.pending(),
                    "in_progress": board.in_progress(),
                    "done": board.done(),
                    "failed": board.failed()
                }
            });
            println!("{json}");
        }
    }
    Ok(())
}

/// Shows details for a single task.
pub async fn show_task(root: &Path, id: i64, format: Format) -> Result<()> {
    let conn = do_harness_db::connect_and_migrate(root).await?;
    let task = do_harness_db::get_task(&conn, id)
        .await?
        .with_context(|| format!("task {id} not found"))?;
    match format {
        Format::Text => {
            println!("Task {}: {}", task.id, task.title);
            println!("  status: {}", task.status.as_str());
            println!("  method: {}", task.method.as_deref().unwrap_or("-"));
            println!("  subtask_index: {}", task.subtask_index);
            println!(
                "  parent_id: {}",
                task.parent_id.map_or("-".to_owned(), |p| p.to_string())
            );
            println!(
                "  precondition: {}",
                task.precondition.as_deref().unwrap_or("-")
            );
        }
        Format::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&task).context("failed to serialize task")?
            );
        }
    }
    Ok(())
}

mod mutations;

// The lifecycle commands live in `mutations`; re-exported so `do-harness task`
// call sites and the test modules keep addressing them as `task::<name>`.
pub use mutations::{add_task, advance_task, done_task, fail_task, remove_task};

#[cfg(test)]
mod export_tests;
#[cfg(test)]
mod hygiene_tests;
#[cfg(test)]
mod tests;

#[cfg(test)]
mod workflow_tests;

#[cfg(test)]
mod gate_tests;
