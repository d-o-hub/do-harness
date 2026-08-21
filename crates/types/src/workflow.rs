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

/// Lifecycle state a task can occupy on the board, derived from the last
/// event observed for its id.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BoardState {
    Pending,
    InProgress,
    Done,
    Failed,
}

/// Read-side projection folding [`WorkflowEvent`]s into board counts.
///
/// State is keyed by task id: each task's entry reflects the latest event
/// seen for it, so replaying a stream is idempotent (duplicate events do not
/// double-count) and folding order across different tasks does not matter.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TaskBoard {
    states: std::collections::BTreeMap<i64, BoardState>,
}

impl TaskBoard {
    /// A freshly reset board with no tasks.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Number of tasks currently in `pending` state.
    #[must_use]
    pub fn pending(&self) -> i64 {
        self.count(BoardState::Pending)
    }

    /// Number of tasks currently in `in_progress` state.
    #[must_use]
    pub fn in_progress(&self) -> i64 {
        self.count(BoardState::InProgress)
    }

    /// Number of tasks currently in `done` state.
    #[must_use]
    pub fn done(&self) -> i64 {
        self.count(BoardState::Done)
    }

    /// Number of tasks currently in `failed` state.
    #[must_use]
    pub fn failed(&self) -> i64 {
        self.count(BoardState::Failed)
    }

    /// The four board counts in `(pending, in_progress, done, failed)` order.
    #[must_use]
    pub fn to_counts(&self) -> (i64, i64, i64, i64) {
        (
            self.pending(),
            self.in_progress(),
            self.done(),
            self.failed(),
        )
    }

    fn count(&self, state: BoardState) -> i64 {
        i64::try_from(
            self.states
                .values()
                .filter(|entry| **entry == state)
                .count(),
        )
        .unwrap_or(i64::MAX)
    }
}

impl Projection for TaskBoard {
    type Event = WorkflowEvent;

    fn apply(&mut self, event: &Self::Event) -> Result<(), ProjectionError> {
        let (id, state) = match event {
            Self::Event::TaskAdded(inner) => (inner.id, BoardState::Pending),
            Self::Event::TaskAdvanced(inner) => (inner.id, BoardState::InProgress),
            Self::Event::TaskCompleted(inner) => (inner.id, BoardState::Done),
            Self::Event::TaskFailed(inner) => (inner.id, BoardState::Failed),
        };
        self.states.insert(id, state);
        Ok(())
    }
}
#[cfg(test)]
mod tests;
