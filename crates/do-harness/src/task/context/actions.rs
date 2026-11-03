//! Next-action selection and task-scoped failure/trace reads.

use anyhow::Result;
use do_harness_types::{TaskRecord, TaskState};

use super::workspace::clean;
use super::{
    Bounded, Evidence, FailureItem, Gating, MAX_FAILURES, MAX_FIELD_CHARS, MAX_TRACES, NextAction,
    StepView, TraceItem,
};

/// Freshness summary plus a recovery command.
pub(super) fn evidence_view(gating: &Gating, id: i64) -> Evidence {
    let recovery = match gating.reason {
        "no_sensor_gate"
        | "task_closed"
        | "no_method"
        | "method_missing"
        | "method_policy_unreadable"
        | "invalid_index"
        | "policy_unreadable" => None,
        _ => Some(recovery_command(gating, id)),
    };
    Evidence {
        state: gating.state,
        reason: gating.reason,
        recovery,
    }
}

/// Re-record command for the sensors the current gate requires.
fn recovery_command(gating: &Gating, id: i64) -> String {
    let unknown = gating
        .sensors
        .iter()
        .any(|s| s.reason == "unknown_sensor" || s.reason == "inputs_unresolved");
    match gating.required_sensors.as_slice() {
        [sensor] if !unknown => {
            format!("do-harness verify --only {sensor} --record --task {id}")
        }
        _ => format!("do-harness verify --record --task {id}"),
    }
}

/// Chooses the single next action from observable state.
pub(super) fn next_action(
    task: &TaskRecord,
    step: &StepView,
    gating: &Gating,
    evidence: &Evidence,
    id: i64,
) -> NextAction {
    if matches!(task.status, TaskState::Done | TaskState::Failed) {
        return NextAction {
            kind: "none",
            command: None,
            reason: "task_closed",
        };
    }
    match step.state {
        "no_method" => {
            return NextAction {
                kind: "configure_method",
                command: None,
                reason: "no_method",
            };
        }
        "method_missing" => {
            return NextAction {
                kind: "fix_method",
                command: None,
                reason: "method_missing",
            };
        }
        "policy_unreadable" => {
            return NextAction {
                kind: "fix_policy",
                command: None,
                reason: "method_policy_unreadable",
            };
        }
        "invalid_index" => {
            return NextAction {
                kind: "fix_state",
                command: None,
                reason: "invalid_index",
            };
        }
        "complete" => {
            return NextAction {
                kind: "done",
                command: Some(format!("do-harness task done {id}")),
                reason: "gates_complete",
            };
        }
        _ => {}
    }
    if gating.state == "not_applicable" {
        return NextAction {
            kind: "advance",
            command: Some(format!("do-harness task advance {id}")),
            reason: "no_sensor_gate",
        };
    }
    if evidence.reason == "policy_unreadable" {
        return NextAction {
            kind: "fix_policy",
            command: None,
            reason: "policy_unreadable",
        };
    }
    match evidence.state {
        "green" => NextAction {
            kind: "advance",
            command: Some(format!("do-harness task advance {id}")),
            reason: "sensor_passing",
        },
        "red" => NextAction {
            kind: "verify",
            command: evidence.recovery.clone(),
            reason: "sensor_failed",
        },
        "stale" => NextAction {
            kind: "verify",
            command: evidence.recovery.clone(),
            reason: "evidence_stale",
        },
        _ => NextAction {
            kind: "verify",
            command: evidence.recovery.clone(),
            reason: "evidence_missing",
        },
    }
}

/// Reads unresolved failures scoped to this task only.
pub(super) async fn failures(
    conn: &do_harness_db::Connection,
    scope: &str,
) -> Result<Bounded<FailureItem>> {
    let mut signatures = do_harness_db::list_error_signatures(conn, Some(scope)).await?;
    signatures.sort_by(|a, b| {
        b.attempt_count
            .cmp(&a.attempt_count)
            .then_with(|| a.signature.cmp(&b.signature))
            .then_with(|| a.scope.cmp(&b.scope))
    });
    let total = signatures.len();
    let items = signatures
        .into_iter()
        .take(MAX_FAILURES)
        .map(|sig| FailureItem {
            signature: clean(&sig.signature, MAX_FIELD_CHARS),
            attempt_count: sig.attempt_count,
            message: sig.message.as_deref().map(|m| clean(m, MAX_FIELD_CHARS)),
            created_at: sig.created_at,
        })
        .collect();
    Ok(Bounded::new(items, total))
}

/// Reads traces owned by this task, newest first.
pub(super) async fn traces(
    conn: &do_harness_db::Connection,
    id: i64,
) -> Result<Bounded<TraceItem>> {
    let total = do_harness_db::count_traces_for_task(conn, id).await?;
    let total = usize::try_from(total.max(0)).unwrap_or(0);
    let limit = u32::try_from(MAX_TRACES).unwrap_or(u32::MAX);
    let rows = do_harness_db::list_traces_for_task(conn, id, Some(limit)).await?;
    let items = rows
        .into_iter()
        .map(|trace| TraceItem {
            id: trace.id,
            command: trace.command.as_deref().map(|c| clean(c, MAX_FIELD_CHARS)),
            error_diff: trace
                .error_diff
                .as_deref()
                .map(|d| clean(d, MAX_FIELD_CHARS)),
            resolution_steps: trace
                .resolution_steps
                .as_deref()
                .map(|s| clean(s, MAX_FIELD_CHARS)),
            created_at: trace.created_at,
        })
        .collect();
    Ok(Bounded::new(items, total))
}
