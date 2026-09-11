//! Task state queries and exports for `do-harness task`.

use std::fmt::Write;
use std::path::Path;

use anyhow::{Context, Result};
use do_harness_types::{Beat, Projection, TaskBoard, TaskRecord, TaskState, WorkflowEvent};
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
}

/// Writes `plans/tasks.json` or custom output with the task list; returns task count.
pub async fn export_tasks(
    root: &Path,
    output: Option<&Path>,
    stdout: bool,
    format: Format,
) -> Result<usize> {
    let conn = do_harness_db::connect_and_migrate(root).await?;
    let tasks = do_harness_db::list_tasks(&conn).await?;
    let snapshot = TaskSnapshot {
        exported_at: do_harness_db::unix_now(),
        tasks,
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
    tokio::fs::write(&target_path, content)
        .await
        .with_context(|| format!("failed to write {}", target_path.display()))?;
    Ok(snapshot.tasks.len())
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

/// Removes a task from the state database.
pub async fn remove_task(root: &Path, id: i64) -> Result<()> {
    let conn = do_harness_db::connect_and_migrate(root).await?;
    let deleted = conn.execute("DELETE FROM tasks WHERE id = ?", [id]).await?;
    if deleted == 0 {
        anyhow::bail!("task {id} not found");
    }
    println!("Removed task {id}");
    Ok(())
}

/// Inserts a new task in `pending` state with `subtask_index = 0`.
pub async fn add_task(
    root: &Path,
    title: &str,
    method: Option<&str>,
    parent_id: Option<i64>,
    precondition: Option<&str>,
) -> Result<(i64, WorkflowEvent)> {
    if title.trim().is_empty() {
        anyhow::bail!("task title cannot be empty");
    }
    let conn = do_harness_db::connect_and_migrate(root).await?;
    if let Some(method_name) = method {
        let methods = crate::methods::load_methods(root).await?;
        if crate::methods::find_method(&methods, method_name).is_none() {
            anyhow::bail!("unknown method '{method_name}': not in plans/methods.json");
        }
    }
    if let Some(parent) = parent_id {
        if do_harness_db::get_task(&conn, parent).await?.is_none() {
            anyhow::bail!("parent task {parent} not found");
        }
    }
    do_harness_db::insert_task_with_event(
        &conn,
        &do_harness_db::NewTask {
            title,
            method,
            subtask_index: 0,
            precondition,
            parent_id,
        },
    )
    .await
    .map_err(anyhow::Error::from)
}

/// Advances the subtask pointer of a task.
pub async fn advance_task(root: &Path, id: i64) -> Result<(i64, WorkflowEvent)> {
    let conn = do_harness_db::connect_and_migrate(root).await?;
    let task = do_harness_db::get_task(&conn, id)
        .await?
        .with_context(|| format!("task {id} not found"))?;
    if matches!(task.status, TaskState::Done | TaskState::Failed) {
        anyhow::bail!("task {id} is {}; cannot advance", task.status.as_str());
    }
    let Some(method_name) = task.method else {
        anyhow::bail!("task {id} has no method; cannot advance");
    };
    let idx = usize::try_from(task.subtask_index)
        .with_context(|| format!("task {id} has an invalid subtask_index"))?;
    let methods = crate::methods::load_methods(root).await?;
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
    do_harness_db::advance_subtask_with_event(&conn, id, method.subtasks[idx].sensor.as_deref())
        .await
        .map_err(|err| match err {
            do_harness_db::DbError::GateUnsatisfied { sensor, .. } => anyhow::anyhow!(
                "cannot advance task {id}: subtask '{}' requires sensor '{sensor}' to pass (run: do-harness verify --record --task {id})",
                method.subtasks[idx].name
            ),
            other => other.into(),
        })
}

fn latest_sensor_beat_ok(beats: &[Beat], sensor: &str) -> bool {
    beats
        .iter()
        .rev()
        .find(|beat| beat.beat_type == "sensor" && beat.sensor_name.as_deref() == Some(sensor))
        .is_some_and(|beat| beat.status == "ok")
}

/// Marks a task as done once all sensor-gated subtasks have passed.
pub async fn done_task(root: &Path, id: i64) -> Result<WorkflowEvent> {
    let conn = do_harness_db::connect_and_migrate(root).await?;
    let task = do_harness_db::get_task(&conn, id)
        .await?
        .with_context(|| format!("task {id} not found"))?;
    let Some(method_name) = task.method else {
        anyhow::bail!("task {id} has no method; cannot mark done");
    };
    let methods = crate::methods::load_methods(root).await?;
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
    let required_sensors: Vec<String> = method
        .subtasks
        .iter()
        .take(index.min(method.subtasks.len()))
        .filter_map(|sub| sub.sensor.clone())
        .collect();
    do_harness_db::update_task_status_with_event(&conn, id, TaskState::Done, &required_sensors)
        .await
        .map_err(|err| match err {
            do_harness_db::DbError::GateUnsatisfied { sensor, .. } => anyhow::anyhow!(
                "cannot mark task {id} done: sensor '{sensor}' has no passing beat (run: do-harness verify --record --task {id})"
            ),
            other => other.into(),
        })
}

/// Marks a task as failed.
pub async fn fail_task(root: &Path, id: i64) -> Result<WorkflowEvent> {
    let conn = do_harness_db::connect_and_migrate(root).await?;
    if do_harness_db::get_task(&conn, id).await?.is_none() {
        anyhow::bail!("task {id} not found");
    }
    do_harness_db::update_task_status_with_event(&conn, id, TaskState::Failed, &[])
        .await
        .map_err(anyhow::Error::from)
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod workflow_tests;

#[cfg(test)]
mod gate_tests;
