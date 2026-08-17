//! Typed workflow commands, events, and the task-board projection.
//!
//! This module gives the event-sourcing base contracts in [`crate::event`]
//! real call sites for the workflow domain: write-side [`Command`]s are
//! handled by the CLI and emit immutable [`DomainEvent`]s, which a
//! read-side [`Projection`] folds into a [`TaskBoard`] view. This is
//! type-level modeling only; there is no event-store table.

use serde::{Deserialize, Serialize};

use crate::event::{Command, DomainEvent, Projection, ProjectionError};

/// Command to insert a new task in `pending` state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AddTask {
    /// Human-readable task title.
    pub title: String,
    /// Name of the HTN method this task follows, when given.
    pub method: Option<String>,
    /// Parent task id for the hierarchical task network.
    pub parent_id: Option<i64>,
    /// Recorded precondition guard the task is created under.
    pub precondition: Option<String>,
}

impl Command for AddTask {
    fn name(&self) -> &'static str {
        "AddTask"
    }
}

/// Command to advance a task's subtask pointer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdvanceTask {
    /// Task id to advance.
    pub task_id: i64,
}

impl Command for AdvanceTask {
    fn name(&self) -> &'static str {
        "AdvanceTask"
    }
}

/// Command to mark a task done.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompleteTask {
    /// Task id to mark done.
    pub task_id: i64,
}

impl Command for CompleteTask {
    fn name(&self) -> &'static str {
        "CompleteTask"
    }
}

/// Command to fail a task.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FailTask {
    /// Task id to mark failed.
    pub task_id: i64,
}

impl Command for FailTask {
    fn name(&self) -> &'static str {
        "FailTask"
    }
}

/// Event emitted after a task is inserted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskAdded {
    /// The newly assigned task id.
    pub id: i64,
    /// Human-readable task title.
    pub title: String,
    /// Name of the HTN method this task follows, when given.
    pub method: Option<String>,
}

impl DomainEvent for TaskAdded {
    fn name(&self) -> &'static str {
        "TaskAdded"
    }
}

/// Event emitted after a task's subtask pointer advanced.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskAdvanced {
    /// The advanced task id.
    pub id: i64,
    /// The new `subtask_index` after advancing.
    pub subtask_index: i64,
}

impl DomainEvent for TaskAdvanced {
    fn name(&self) -> &'static str {
        "TaskAdvanced"
    }
}

/// Event emitted after a task is marked done.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskCompleted {
    /// The completed task id.
    pub id: i64,
}

impl DomainEvent for TaskCompleted {
    fn name(&self) -> &'static str {
        "TaskCompleted"
    }
}

/// Event emitted after a task is marked failed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskFailed {
    /// The failed task id.
    pub id: i64,
}

impl DomainEvent for TaskFailed {
    fn name(&self) -> &'static str {
        "TaskFailed"
    }
}

/// The complete workflow event stream for the task domain.
///
/// Wrapping all concrete events lets a single projection consume every event
/// type while preserving the closed type-safe set of immutable facts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum WorkflowEvent {
    /// A task was inserted.
    TaskAdded(TaskAdded),
    /// A task advanced a subtask.
    TaskAdvanced(TaskAdvanced),
    /// A task was marked done.
    TaskCompleted(TaskCompleted),
    /// A task was marked failed.
    TaskFailed(TaskFailed),
}

impl DomainEvent for WorkflowEvent {
    fn name(&self) -> &'static str {
        match self {
            Self::TaskAdded(inner) => inner.name(),
            Self::TaskAdvanced(inner) => inner.name(),
            Self::TaskCompleted(inner) => inner.name(),
            Self::TaskFailed(inner) => inner.name(),
        }
    }
}

/// Read-side projection folding [`WorkflowEvent`]s into board counts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TaskBoard {
    /// Number of tasks in `pending` state.
    pub pending: i64,
    /// Number of tasks in `in_progress` state.
    pub in_progress: i64,
    /// Number of tasks in `done` state.
    pub done: i64,
    /// Number of tasks in `failed` state.
    pub failed: i64,
}

impl TaskBoard {
    /// A freshly reset board with every counter at zero.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            pending: 0,
            in_progress: 0,
            done: 0,
            failed: 0,
        }
    }

    /// The four board counts in `(pending, in_progress, done, failed)` order.
    #[must_use]
    pub const fn to_counts(self) -> (i64, i64, i64, i64) {
        (self.pending, self.in_progress, self.done, self.failed)
    }
}

impl Projection for TaskBoard {
    type Event = WorkflowEvent;

    fn apply(&mut self, event: &Self::Event) -> Result<(), ProjectionError> {
        match event {
            Self::Event::TaskAdded(_) => self.pending += 1,
            Self::Event::TaskAdvanced(_) => self.in_progress += 1,
            Self::Event::TaskCompleted(_) => self.done += 1,
            Self::Event::TaskFailed(_) => self.failed += 1,
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fresh board starts with every counter at zero.
    #[test]
    fn task_board_starts_empty() {
        assert_eq!(TaskBoard::new().to_counts(), (0, 0, 0, 0));
    }

    /// Folding a stream updates each counter once per matching event.
    #[test]
    fn task_board_folds_events() {
        let mut board = TaskBoard::new();
        board
            .apply(&WorkflowEvent::TaskAdded(TaskAdded {
                id: 1,
                title: "a".to_owned(),
                method: None,
            }))
            .unwrap();
        board
            .apply(&WorkflowEvent::TaskAdded(TaskAdded {
                id: 2,
                title: "b".to_owned(),
                method: None,
            }))
            .unwrap();
        board
            .apply(&WorkflowEvent::TaskAdvanced(TaskAdvanced {
                id: 2,
                subtask_index: 1,
            }))
            .unwrap();
        board
            .apply(&WorkflowEvent::TaskCompleted(TaskCompleted { id: 1 }))
            .unwrap();
        board
            .apply(&WorkflowEvent::TaskFailed(TaskFailed { id: 3 }))
            .unwrap();

        assert_eq!(board.to_counts(), (2, 1, 1, 1));
    }

    /// Commands expose their stable names.
    #[test]
    fn command_names() {
        assert_eq!(
            AddTask {
                title: "t".to_owned(),
                method: None,
                parent_id: None,
                precondition: None,
            }
            .name(),
            "AddTask"
        );
        assert_eq!(AdvanceTask { task_id: 1 }.name(), "AdvanceTask");
        assert_eq!(CompleteTask { task_id: 1 }.name(), "CompleteTask");
        assert_eq!(FailTask { task_id: 1 }.name(), "FailTask");
    }

    /// The wrapper event delegates its name to the inner event.
    #[test]
    fn wrapped_event_delegates_name() {
        let event = WorkflowEvent::TaskAdded(TaskAdded {
            id: 1,
            title: "t".to_owned(),
            method: None,
        });
        assert_eq!(event.name(), "TaskAdded");
    }

    /// Concrete events round-trip through JSON.
    #[test]
    fn events_roundtrip_json() {
        let event = WorkflowEvent::TaskAdvanced(TaskAdvanced {
            id: 7,
            subtask_index: 2,
        });
        let json = serde_json::to_string(&event).unwrap();
        let parsed: WorkflowEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, event);
    }

    /// A projection rejects an event outside its stream with `UnsupportedEvent`.
    #[test]
    fn projection_rejects_unmatched_event() {
        /// Counter restricted to `TaskAdded` events only.
        #[derive(Debug, Default)]
        struct AddedCount {
            n: i64,
        }

        impl Projection for AddedCount {
            type Event = WorkflowEvent;

            fn apply(&mut self, event: &Self::Event) -> Result<(), ProjectionError> {
                match event {
                    Self::Event::TaskAdded(_) => {
                        self.n += 1;
                        Ok(())
                    }
                    other => Err(ProjectionError::UnsupportedEvent(other.name())),
                }
            }
        }

        let mut board = AddedCount::default();
        board
            .apply(&WorkflowEvent::TaskAdded(TaskAdded {
                id: 1,
                title: "t".to_owned(),
                method: None,
            }))
            .unwrap();
        let err = board
            .apply(&WorkflowEvent::TaskAdvanced(TaskAdvanced {
                id: 1,
                subtask_index: 1,
            }))
            .unwrap_err();
        assert!(matches!(
            err,
            ProjectionError::UnsupportedEvent("TaskAdvanced")
        ));
        assert_eq!(
            err.to_string(),
            "event TaskAdvanced cannot be applied to this projection"
        );
    }
}
