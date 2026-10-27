//! Filesystem snapshot evaluated inside the task writer's database transaction.
use std::path::Path;

use do_harness_db::{DbError, FreshTaskInputs};

pub(super) fn snapshot(
    root: &Path,
    task: &do_harness_types::TaskRecord,
    sensors: &[String],
    complete: bool,
) -> do_harness_db::Result<FreshTaskInputs> {
    let bytes = match std::fs::read(root.join("do-harness.toml")) {
        Ok(bytes) => Some(bytes),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(_) => return Err(DbError::Invariant("cannot read verification policy".into())),
    };
    let cfg = if let Some(bytes) = &bytes {
        toml::from_str::<crate::config::Config>(
            std::str::from_utf8(bytes)
                .map_err(|_| DbError::Invariant("invalid verification policy encoding".into()))?,
        )
        .map_err(|_| DbError::Invariant("invalid verification policy".into()))?
    } else {
        crate::config::rust_default()
    };
    cfg.validate()
        .map_err(|_| DbError::Invariant("invalid verification policy".into()))?;
    let methods = crate::methods::load_methods_sync(root)
        .map_err(|_| DbError::Invariant("cannot establish current method policy".into()))?;
    if let Some(name) = &task.method {
        let method = crate::methods::find_method(&methods, name)
            .ok_or_else(|| DbError::InvalidTaskTransition("method policy changed".into()))?;
        let index = usize::try_from(task.subtask_index)
            .map_err(|_| DbError::InvalidTaskTransition("invalid task index".into()))?;
        let expected: Vec<String> = if complete {
            let last = method.subtasks.iter().rposition(|s| s.sensor.is_some());
            if index < method.subtasks.len() && last.is_none_or(|last| index <= last) {
                return Err(DbError::InvalidTaskTransition(
                    "sensor-gated subtasks remain".into(),
                ));
            }
            method
                .subtasks
                .iter()
                .take(index)
                .filter_map(|s| s.sensor.clone())
                .collect()
        } else {
            let subtask = method
                .subtasks
                .get(index)
                .ok_or_else(|| DbError::InvalidTaskTransition("no subtask remains".into()))?;
            subtask.sensor.iter().cloned().collect()
        };
        if expected != sensors {
            return Err(DbError::InvalidTaskTransition(
                "required sensors changed".into(),
            ));
        }
    }
    let policy_fingerprint = crate::fingerprint::hash_canonical(&serde_json::json!({
        "config": crate::fingerprint::config_digest(bytes.as_deref()), "methods": methods,
    }));
    let identities = sensors
        .iter()
        .map(|sensor| {
            let digest = cfg
                .effective_sensors()
                .iter()
                .find(|s| &s.name == sensor)
                .and_then(|s| crate::provenance::sensor_digest(root, s, bytes.as_deref()))
                .ok_or_else(|| DbError::GateUnsatisfied {
                    task_id: task.id,
                    sensor: sensor.clone(),
                })?;
            Ok((sensor.clone(), digest))
        })
        .collect::<do_harness_db::Result<Vec<_>>>()?;
    Ok(FreshTaskInputs {
        method: task.method.clone(),
        subtask_index: task.subtask_index,
        policy_fingerprint,
        sensors: identities,
    })
}

pub(super) async fn preview(
    root: &Path,
    conn: &do_harness_db::Connection,
    task: &do_harness_types::TaskRecord,
    sensors: &[String],
    complete: bool,
) -> anyhow::Result<()> {
    let inputs = snapshot(root, task, sensors, complete)?;
    for (sensor, digest) in inputs.sensors {
        let beat =
            do_harness_db::latest_sensor_beat(conn, &sensor, &format!("task:{}", task.id)).await?;
        if !beat.is_some_and(|b| {
            b.status.is_ok()
                && b.exit_code == Some(0)
                && b.input_digest.as_deref().is_some_and(|stored| {
                    stored
                        .strip_prefix(do_harness_db::OBSERVATION_ONLY_PREFIX)
                        .unwrap_or(stored)
                        == digest
                })
        }) {
            anyhow::bail!("sensor '{sensor}' has no fresh passing beat");
        }
    }
    Ok(())
}
