//! Task lifecycle commands: add, advance, done, fail, and remove.
//!
//! Split out of `task.rs` so neither file approaches the 500-line ceiling, and
//! because these are the only task operations that write workflow events.

use std::path::Path;

use anyhow::{Context, Result};
use do_harness_types::{Beat, TaskState, WorkflowEvent};

/// Removes a task from the state database.
///
/// A task that has an event trail cannot be deleted: `workflow_events` is
/// append-only and hash-chained, so removing its rows would destroy audit
/// evidence and break the chain (`audit-chain`) for every later event. Deleting
/// only the `tasks` row is impossible for the same reason — the
/// `workflow_events.task_id` foreign key has no cascade, which is what the raw
/// `FOREIGN KEY constraint failed` error used to surface.
///
/// Every task created through `task add` writes a `TaskAdded` event, so in
/// practice `remove` only deletes orphan rows (the out-of-band case `doctor`
/// already flags). For a real task the supported transition is `task fail`,
/// which appends `TaskFailed { id }` — a terminal transition that leaves the
/// audit trail intact, though it records no actor or reason. A first-class
/// "cancelled" state is not modelled: adding one means a new `WorkflowEvent`
/// variant, a new `TaskState`, and a projection change, so it is deliberately
/// not faked here.
pub async fn remove_task(root: &Path, id: i64) -> Result<()> {
    let conn = do_harness_db::connect_and_migrate(root).await?;
    if do_harness_db::get_task(&conn, id).await?.is_none() {
        anyhow::bail!("task {id} not found");
    }
    let mut rows = conn
        .query(
            "SELECT COUNT(*) FROM workflow_events WHERE task_id = ?",
            [id],
        )
        .await?;
    let events: i64 = match rows.next().await? {
        Some(row) => row.get(0)?,
        None => 0,
    };
    if events > 0 {
        anyhow::bail!(
            "task {id} has {events} workflow event(s) and cannot be removed: the event log is \
             append-only, so deleting it would destroy audit evidence. Use `task fail {id}` to \
             close it with a recorded transition, or `task done {id}` once its sensor gates pass."
        );
    }
    let deleted = conn.execute("DELETE FROM tasks WHERE id = ?", [id]).await?;
    if deleted == 0 {
        anyhow::bail!("task {id} not found");
    }
    println!("Removed orphan task {id} (no event trail)");
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
