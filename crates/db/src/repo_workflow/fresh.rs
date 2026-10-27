//! Task transitions that consume fresh input identities within the write transaction.
use do_harness_types::{TaskAdvanced, TaskCompleted, TaskState, WorkflowEvent};
use libsql::Connection;

use crate::{DbError, Result};

/// Prefix for an observed identity that must never authorize cached reuse.
pub const OBSERVATION_ONLY_PREFIX: &str = "observed:";
const BUSY_RETRIES: usize = 3;

/// A transition with a freshness gate. Failing a task needs no passing evidence.
#[derive(Clone, Copy)]
pub enum FreshTransition {
    /// Advance a nonterminal task's next subtask.
    Advance,
    /// Complete a nonterminal task.
    Complete,
}

/// Check canonical current inputs and latest named beats in the same immediate
/// transaction as the task/event update. `snapshot` is evaluated twice inside
/// the transaction; it must return the method/index and required sensor identities.
/// External worktree writes are not `SQLite` transactions: observed drift fails
/// closed, but callers must prevent hostile concurrent filesystem mutation.
///
/// # Errors
/// Returns an error for stale/missing evidence, drift, a concurrent task change,
/// an invalid transition, or a database failure. A failed check rolls back.
pub async fn transition_with_freshness<F>(
    conn: &Connection,
    id: i64,
    transition: FreshTransition,
    snapshot: F,
) -> Result<(i64, WorkflowEvent)>
where
    F: Fn() -> Result<FreshTaskInputs>,
{
    crate::retry_on_busy(BUSY_RETRIES, || async {
        let tx = crate::tx::begin_immediate(conn).await?;
        let before = snapshot()?;
        let task = crate::get_task(&tx, id)
            .await?
            .ok_or_else(|| DbError::NotFound(format!("task {id} not found")))?;
        if task.method != before.method || task.subtask_index != before.subtask_index {
            return Err(DbError::InvalidTaskTransition(
                "task changed during validation".into(),
            ));
        }
        for (sensor, digest) in &before.sensors {
            let beat = crate::latest_sensor_beat(&tx, sensor, &format!("task:{id}")).await?;
            if !beat.is_some_and(|b| {
                b.status.is_ok()
                    && b.exit_code == Some(0)
                    && b.input_digest.as_deref().is_some_and(|stored| {
                        stored
                            .strip_prefix(OBSERVATION_ONLY_PREFIX)
                            .unwrap_or(stored)
                            == digest
                    })
            }) {
                return Err(DbError::GateUnsatisfied {
                    task_id: id,
                    sensor: sensor.clone(),
                });
            }
        }
        let (index, event) = match transition {
            FreshTransition::Advance => {
                let index = crate::repo::advance_subtask(&tx, id).await?;
                (
                    index,
                    WorkflowEvent::TaskAdvanced(TaskAdvanced {
                        id,
                        subtask_index: index,
                    }),
                )
            }
            FreshTransition::Complete => {
                crate::repo::update_task_status(&tx, id, TaskState::Done).await?;
                (
                    task.subtask_index,
                    WorkflowEvent::TaskCompleted(TaskCompleted { id }),
                )
            }
        };
        super::append_event_on(&tx, id, &event).await?;
        if before != snapshot()? {
            return Err(DbError::Invariant(
                "verification inputs changed during task transition".into(),
            ));
        }
        tx.commit().await?;
        Ok((index, event))
    })
    .await
}

/// Safe current identities and the task version used to select its method gates.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FreshTaskInputs {
    /// Expected method name.
    pub method: Option<String>,
    /// Expected current subtask index.
    pub subtask_index: i64,
    /// Policy identity rechecked even for transitions without named sensors.
    pub policy_fingerprint: String,
    /// Named sensors and canonical digests (no raw input values).
    pub sensors: Vec<(String, String)>,
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]
    use super::*;
    use std::cell::Cell;

    #[tokio::test(flavor = "current_thread")]
    async fn a_snapshot_race_rolls_back_the_task_and_event_together() {
        let dir = tempfile::tempdir().unwrap();
        let conn = crate::connect_and_migrate(dir.path()).await.unwrap();
        let (id, _) = crate::insert_task_with_event(
            &conn,
            &crate::NewTask {
                title: "race",
                method: None,
                subtask_index: 0,
                precondition: None,
                parent_id: None,
            },
        )
        .await
        .unwrap();
        let calls = Cell::new(0);
        let result = transition_with_freshness(&conn, id, FreshTransition::Complete, || {
            calls.set(calls.get() + 1);
            Ok(FreshTaskInputs {
                method: None,
                policy_fingerprint: "policy".into(),
                subtask_index: i64::from(calls.get() > 1),
                sensors: Vec::new(),
            })
        })
        .await;
        assert!(result.is_err());
        assert_eq!(
            crate::get_task(&conn, id).await.unwrap().unwrap().status,
            TaskState::Pending
        );
        assert_eq!(crate::list_all_events(&conn).await.unwrap().len(), 1);
    }
}
