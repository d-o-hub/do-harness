//! Typed observations for optional task transition guards.

use libsql::Connection;

use super::OBSERVATION_ONLY_PREFIX;
use crate::{DbError, LatestSensorBeat, Result};

/// Current inputs for a configured `sensor.fresh_pass` predicate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskGuardInput {
    /// The registered sensor to read from the current task's beat history.
    pub sensor: String,
    /// The current input identity, or `None` if it could not be established.
    pub digest: Option<String>,
    /// Whether a rejected or unknown verdict blocks the transition.
    pub enforce: bool,
}

/// Outcome of a guard over the observed task evidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskGuardVerdict {
    /// A current successful run establishes the predicate.
    Satisfied,
    /// A current failing run establishes that the predicate is false.
    Rejected,
    /// Current evidence is missing, stale, or does not establish an outcome.
    Unknown,
}

/// Diagnostic for a configured guard.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskGuardDecision {
    /// The evaluated sensor name.
    pub sensor: String,
    /// Whether this guard is enforced.
    pub enforce: bool,
    /// The observed outcome.
    pub verdict: TaskGuardVerdict,
    /// Stable machine-readable reason.
    pub reason: &'static str,
}

fn evaluate(
    digest: Option<&str>,
    beat: Option<&LatestSensorBeat>,
) -> (TaskGuardVerdict, &'static str) {
    use TaskGuardVerdict::{Rejected, Satisfied, Unknown};
    let Some(digest) = digest else {
        return (Unknown, "inputs_unavailable");
    };
    let Some(beat) = beat else {
        return (Unknown, "evidence_missing");
    };
    let Some(stored) = beat.input_digest.as_deref() else {
        return (Unknown, "evidence_unbound");
    };
    if stored
        .strip_prefix(OBSERVATION_ONLY_PREFIX)
        .unwrap_or(stored)
        != digest
    {
        return (Unknown, "evidence_stale");
    }
    match beat.exit_code {
        Some(0) if beat.status.is_ok() => (Satisfied, "fresh_pass"),
        Some(code) if code != 0 && !beat.status.is_ok() => (Rejected, "fresh_failure"),
        _ => (Unknown, "outcome_unavailable"),
    }
}

/// Evaluate task-scoped guard evidence without changing the task.
///
/// # Errors
/// Returns an error on database failure or if an enforced predicate cannot pass.
pub async fn check_task_guards(
    conn: &Connection,
    id: i64,
    inputs: &[TaskGuardInput],
) -> Result<Vec<TaskGuardDecision>> {
    let mut decisions = Vec::with_capacity(inputs.len());
    let scope = format!("task:{id}");
    for input in inputs {
        let beat = crate::latest_sensor_beat(conn, &input.sensor, &scope).await?;
        let (verdict, reason) = evaluate(input.digest.as_deref(), beat.as_ref());
        decisions.push(TaskGuardDecision {
            sensor: input.sensor.clone(),
            enforce: input.enforce,
            verdict,
            reason,
        });
    }
    let failures = decisions
        .iter()
        .filter_map(|decision| {
            if !decision.enforce || decision.verdict == TaskGuardVerdict::Satisfied {
                return None;
            }
            let verdict = match decision.verdict {
                TaskGuardVerdict::Rejected => "rejected",
                TaskGuardVerdict::Unknown => "unknown",
                TaskGuardVerdict::Satisfied => return None,
            };
            Some(format!(
                "{} {verdict} ({})",
                decision.sensor, decision.reason
            ))
        })
        .collect::<Vec<_>>();
    if !failures.is_empty() {
        return Err(DbError::InvalidTaskTransition(format!(
            "guard sensor.fresh_pass: {}",
            failures.join(", ")
        )));
    }
    Ok(decisions)
}
