//! Assembly of the task, step, and gating sections.

use std::path::Path;

use anyhow::Result;
use do_harness_types::{TaskRecord, TaskState};

use super::actions::{evidence_view, failures, next_action, traces};
use super::workspace::{clean, ranked_skills, workspace_view};
use super::{
    AcceptanceCriteria, Budget, CONTEXT_SCHEMA_VERSION, ContextDocument, Database, Gating,
    MAX_FIELD_CHARS, SensorFreshness, StepView, TaskIdentity, Unknown,
};

/// Assembles the full projection for an existing task.
pub(super) async fn assemble(
    root: &Path,
    conn: &do_harness_db::Connection,
    database: &Database,
    task: &TaskRecord,
    id: i64,
    max_bytes: usize,
) -> Result<ContextDocument> {
    let scope = format!("task:{id}");
    let (step, gating) = step_and_gating(root, conn, task, id).await?;
    let evidence = evidence_view(&gating, id);
    let next_action = next_action(task, &step, &gating, &evidence, id);
    let failures = failures(conn, &scope).await?;
    let traces = traces(conn, id).await?;
    let workspace = workspace_view(root);

    let query = format!(
        "{} {} {}",
        task.title,
        task.method.as_deref().unwrap_or(""),
        step.name.as_deref().unwrap_or("")
    );
    let (skills, paths, catalog_error) = ranked_skills(root, &query);

    let mut unknowns = Vec::new();
    unknowns.push(Unknown {
        section: "acceptance_criteria",
        reason: "not_recorded",
    });
    if !matches!(step.state, "current" | "closed") {
        unknowns.push(Unknown {
            section: "step",
            reason: step.reason,
        });
    }
    if matches!(gating.state, "unavailable" | "not_applicable") {
        unknowns.push(Unknown {
            section: "gating",
            reason: gating.reason,
        });
    }
    if workspace.discovery_failed {
        unknowns.push(Unknown {
            section: "workspace",
            reason: "discovery_failed",
        });
    }
    if catalog_error.is_some() {
        unknowns.push(Unknown {
            section: "skills",
            reason: "catalog_unreadable",
        });
    }

    Ok(ContextDocument {
        schema_version: CONTEXT_SCHEMA_VERSION,
        budget: Budget { max_bytes },
        database: database.clone(),
        task: Some(TaskIdentity {
            id: task.id,
            title: clean(&task.title, MAX_FIELD_CHARS),
            status: task.status.as_str(),
            parent_id: task.parent_id,
            method: task.method.clone(),
            subtask_index: task.subtask_index,
            precondition: task
                .precondition
                .as_deref()
                .map(|p| clean(p, MAX_FIELD_CHARS)),
        }),
        step: Some(step),
        gating: Some(gating),
        acceptance_criteria: AcceptanceCriteria {
            available: false,
            reason: "not_recorded",
            note: "TaskRecord has no acceptance-criteria field; none are inferred from the title or a passing sensor.",
        },
        evidence,
        failures,
        traces,
        workspace,
        next_action,
        skills,
        paths,
        unknowns,
    })
}

/// Resolves the method step and the sensors the next transition requires.
async fn step_and_gating(
    root: &Path,
    conn: &do_harness_db::Connection,
    task: &TaskRecord,
    id: i64,
) -> Result<(StepView, Gating)> {
    if matches!(task.status, TaskState::Done | TaskState::Failed) {
        return Ok(unresolved(
            "closed",
            "task_closed",
            task.method.clone(),
            task.subtask_index,
            None,
            "not_applicable",
        ));
    }
    let Some(method_name) = task.method.as_deref() else {
        return Ok(unresolved(
            "no_method",
            "no_method",
            None,
            task.subtask_index,
            None,
            "unavailable",
        ));
    };
    let Ok(methods) = crate::methods::load_methods_sync(root) else {
        return Ok(unresolved(
            "policy_unreadable",
            "method_policy_unreadable",
            Some(method_name.to_owned()),
            task.subtask_index,
            None,
            "unavailable",
        ));
    };
    let Some(method) = crate::methods::find_method(&methods, method_name) else {
        return Ok(unresolved(
            "method_missing",
            "method_missing",
            Some(method_name.to_owned()),
            task.subtask_index,
            None,
            "unavailable",
        ));
    };
    let Ok(index) = usize::try_from(task.subtask_index) else {
        return Ok(unresolved(
            "invalid_index",
            "invalid_index",
            Some(method_name.to_owned()),
            task.subtask_index,
            Some(method.subtasks.len()),
            "unavailable",
        ));
    };
    let total = method.subtasks.len();
    let past_all = index >= total;
    let (name, sensor, remaining) = if past_all {
        (None, None, Some(0))
    } else {
        (
            Some(method.subtasks[index].name.clone()),
            method.subtasks[index].sensor.clone(),
            Some(total - index - 1),
        )
    };
    // Same sensor set the `done` gate requires once every subtask is consumed;
    // before that, the current subtask's single sensor is the next gate.
    let required: Vec<String> = if past_all {
        method
            .subtasks
            .iter()
            .take(index.min(total))
            .filter_map(|sub| sub.sensor.clone())
            .collect()
    } else {
        sensor.iter().cloned().collect()
    };
    let sensors = sensor_freshness(root, conn, &required, id).await;
    let (state, reason) = aggregate(&sensors);
    let step = StepView {
        state: if past_all { "complete" } else { "current" },
        method: Some(method_name.to_owned()),
        index: task.subtask_index,
        name,
        sensor,
        total_subtasks: Some(total),
        remaining_subtasks: remaining,
        reason: if past_all {
            "gates_complete"
        } else {
            "current"
        },
    };
    Ok((
        step,
        Gating {
            required_sensors: required,
            sensors,
            state,
            reason,
        },
    ))
}

/// A step/gating pair for a task whose current step cannot be resolved.
fn unresolved(
    state: &'static str,
    reason: &'static str,
    method: Option<String>,
    index: i64,
    total: Option<usize>,
    gating_state: &'static str,
) -> (StepView, Gating) {
    (
        StepView {
            state,
            method,
            index,
            name: None,
            sensor: None,
            total_subtasks: total,
            remaining_subtasks: None,
            reason,
        },
        Gating {
            required_sensors: Vec::new(),
            sensors: Vec::new(),
            state: gating_state,
            reason,
        },
    )
}

/// Computes task-scoped freshness for each required sensor.
async fn sensor_freshness(
    root: &Path,
    conn: &do_harness_db::Connection,
    required: &[String],
    id: i64,
) -> Vec<SensorFreshness> {
    if required.is_empty() {
        return Vec::new();
    }
    let scope = format!("task:{id}");
    let config = crate::config::load_raw(root, None).await.ok();
    let mut out = Vec::with_capacity(required.len());
    for name in required {
        let verdict = match &config {
            None => ("missing", "policy_unreadable"),
            Some((cfg, bytes)) => {
                let spec = cfg.effective_sensors().iter().find(|s| s.name == *name);
                match spec {
                    None => ("missing", "unknown_sensor"),
                    Some(spec) => {
                        let digest = crate::provenance::sensor_digest(root, spec, bytes.as_deref());
                        match digest {
                            None => ("missing", "inputs_unresolved"),
                            Some(digest) => {
                                let beat = do_harness_db::latest_sensor_beat(conn, name, &scope)
                                    .await
                                    .ok()
                                    .flatten();
                                sensor_verdict(&digest, beat.as_ref())
                            }
                        }
                    }
                }
            }
        };
        out.push(SensorFreshness {
            name: name.clone(),
            state: verdict.0,
            reason: verdict.1,
        });
    }
    out
}

/// Freshness verdict for one recorded beat, mirroring the task gate.
fn sensor_verdict(
    digest: &str,
    beat: Option<&do_harness_db::LatestSensorBeat>,
) -> (&'static str, &'static str) {
    let Some(beat) = beat else {
        return ("missing", "no_beat");
    };
    if !beat.status.is_ok() || beat.exit_code != Some(0) {
        return ("red", "failed");
    }
    let recorded = beat.input_digest.as_deref().map(|stored| {
        stored
            .strip_prefix(do_harness_db::OBSERVATION_ONLY_PREFIX)
            .unwrap_or(stored)
    });
    if recorded == Some(digest) {
        ("current", "current")
    } else {
        ("stale", "inputs_changed")
    }
}

/// Aggregates per-sensor states with a fixed precedence.
fn aggregate(sensors: &[SensorFreshness]) -> (&'static str, &'static str) {
    if sensors.is_empty() {
        return ("not_applicable", "no_sensor_gate");
    }
    for (state, reason) in [("red", "failures"), ("stale", "inputs_changed")] {
        if sensors.iter().any(|s| s.state == state) {
            return (state, reason);
        }
    }
    // Preserve the first missing sensor's specific reason rather than
    // flattening every absence to `no_beat`.
    if let Some(missing) = sensors.iter().find(|s| s.state == "missing") {
        return ("missing", missing.reason);
    }
    ("green", "current")
}
