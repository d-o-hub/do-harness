//! Task state queries and exports for `do-harness task`.

use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use do_harness_types::{Beat, TaskRecord, TaskState};
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
/// The method name, when given, must exist in the frozen method catalog; the
/// parent link is persisted when `parent_id` is given, keeping the
/// hierarchical task network intact for later workflow runs. Returns the new
/// task id.
///
/// # Errors
///
/// Returns an error when the state database cannot be opened, when the method
/// is unknown or the parent does not exist, or when the insert fails.
pub async fn add_task(
    root: &Path,
    title: &str,
    method: Option<&str>,
    parent_id: Option<i64>,
    precondition: Option<&str>,
) -> Result<i64> {
    let conn = do_harness_db::connect_and_migrate(root).await?;
    if let Some(method_name) = method {
        let methods = crate::methods::load_methods(root)?;
        if crate::methods::find_method(&methods, method_name).is_none() {
            anyhow::bail!("unknown method '{method_name}': not in plans/methods.json");
        }
    }
    if let Some(parent) = parent_id {
        if do_harness_db::get_task(&conn, parent).await?.is_none() {
            anyhow::bail!("parent task {parent} not found");
        }
    }
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
/// Advancing is gated by the HTN method catalog: when the current subtask
/// declares a computational sensor, a latest `"ok"` sensor beat scoped to
/// this task must exist (`verify --record --task <id>`), and a task that is
/// already `done` or `failed` cannot advance. `advance_subtask` also sets the
/// status to `in_progress`.
///
/// # Errors
///
/// Returns an error when the state database cannot be opened, when no task
/// with the given id exists, when the task is `done`/`failed`, when there are
/// no more subtasks, when the sensor gate has not passed, or when the advance
/// fails.
pub async fn advance_task(root: &Path, id: i64) -> Result<i64> {
    let conn = do_harness_db::connect_and_migrate(root).await?;
    let task = do_harness_db::get_task(&conn, id)
        .await?
        .with_context(|| format!("task {id} not found"))?;
    if matches!(task.status, TaskState::Done | TaskState::Failed) {
        anyhow::bail!("task {id} is {}; cannot advance", task.status.as_str());
    }
    if let Some(method_name) = task.method {
        let idx = usize::try_from(task.subtask_index)
            .with_context(|| format!("task {id} has an invalid subtask_index"))?;
        let methods = crate::methods::load_methods(root)?;
        let method = crate::methods::find_method(&methods, &method_name)
            .with_context(|| format!("task {id} references unknown method '{method_name}'"))?;
        if idx >= method.subtasks.len() {
            anyhow::bail!("task {id} has no more subtasks to advance");
        }
        if let Some(sensor) = &method.subtasks[idx].sensor {
            let beats = do_harness_db::list_beats(&conn, Some(id)).await?;
            if !latest_sensor_beat_ok(&beats, sensor) {
                anyhow::bail!(
                    "cannot advance task {id}: subtask '{}' requires sensor '{sensor}' to pass (run: do-harness verify --record --task {id})",
                    method.subtasks[idx].name
                );
            }
        }
    }
    do_harness_db::advance_subtask(&conn, id).await
}

/// Returns whether the most recent `sensor` beat for this task that matches the
/// named sensor has `status == "ok"`.
///
/// Beats carry `sensor_name` (see migration 0005), so a gate on `check` cannot
/// be satisfied by a passing `fmt` beat. When no `ok` beat is recorded for the
/// named sensor, the gate closes (fails), even if another sensor passed.
fn latest_sensor_beat_ok(beats: &[Beat], sensor: &str) -> bool {
    beats
        .iter()
        .rev()
        .find(|beat| beat.beat_type == "sensor" && beat.sensor_name.as_deref() == Some(sensor))
        .is_some_and(|beat| beat.status == "ok")
}

/// Marks a task as done once all sensor-gated subtasks have passed.
///
/// The task must have a method, and it must have advanced past every
/// sensor-gated subtask (or past the end of the subtask list) before it may
/// be marked done.
///
/// # Errors
///
/// Returns an error when the state database cannot be opened, when no task
/// with the given id exists, when the task has no method, when subtasks
/// remain, or when the status update fails.
pub async fn done_task(root: &Path, id: i64) -> Result<()> {
    let conn = do_harness_db::connect_and_migrate(root).await?;
    let task = do_harness_db::get_task(&conn, id)
        .await?
        .with_context(|| format!("task {id} not found"))?;
    let Some(method_name) = task.method else {
        anyhow::bail!("task {id} has no method; cannot mark done");
    };
    let methods = crate::methods::load_methods(root)?;
    let method = crate::methods::find_method(&methods, &method_name)
        .with_context(|| format!("task {id} references unknown method '{method_name}'"))?;
    let index = usize::try_from(task.subtask_index)
        .with_context(|| format!("task {id} has an invalid subtask_index"))?;
    let last_sensor = method.subtasks.iter().rposition(|sub| sub.sensor.is_some());
    let past_all_sensor_gates =
        index >= method.subtasks.len() || last_sensor.is_some_and(|last| index > last);
    if !past_all_sensor_gates {
        anyhow::bail!(
            "task {id} is not done: subtasks remain (subtask_index {} of {})",
            task.subtask_index,
            method.subtasks.len()
        );
    }
    do_harness_db::update_task_status(&conn, id, TaskState::Done).await
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
mod tests;
