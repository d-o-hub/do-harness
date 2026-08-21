//! Typed Hierarchical Task Network (HTN) structures.
//!
//! Mirrors the `.agents/skills/htn-planner` method catalog so planning state
//! can be persisted and validated as data instead of prose.

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Error returned when a stored or provided task-state string is not one of
/// the four lifecycle values.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("unknown task state '{value}'")]
pub struct TaskStateParseError {
    /// The unrecognized value.
    pub value: String,
}

/// A named HTN decomposition method (e.g., `vertical-event-slice`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Method {
    /// Unique method identifier, lowercase with hyphens.
    pub name: String,
    /// Ordered primitive subtasks that make up the method.
    pub subtasks: Vec<Subtask>,
    /// Guards that must hold before the method may be selected.
    pub preconditions: Vec<Precondition>,
}

/// A primitive, atomic subtask within a method.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Subtask {
    /// Subtask identifier, lowercase with hyphens.
    pub name: String,
    /// The computational sensor that must pass before advancing.
    pub sensor: Option<String>,
    /// Whether this subtask may be delegated to a spike.
    pub spike_candidate: bool,
}

/// A precondition guard evaluated before an action is invoked.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Precondition {
    /// Human-readable description of the guard.
    pub description: String,
}

/// Lifecycle state of a task or subtask in the execution trace.
///
/// Serializes in `snake_case` to match the `tasks.status` column values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "snake_case")]
pub enum TaskState {
    /// Planned but not yet started.
    Pending,
    /// Currently being executed.
    InProgress,
    /// All sensor gates passed; verified complete.
    Done,
    /// Sensor failed repeatedly; halted by the fail-fast policy.
    Failed,
}

impl TaskState {
    /// The `snake_case` database representation of this state.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::InProgress => "in_progress",
            Self::Done => "done",
            Self::Failed => "failed",
        }
    }
}

impl TryFrom<&str> for TaskState {
    type Error = TaskStateParseError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "pending" => Ok(Self::Pending),
            "in_progress" => Ok(Self::InProgress),
            "done" => Ok(Self::Done),
            "failed" => Ok(Self::Failed),
            other => Err(TaskStateParseError {
                value: other.to_owned(),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    /// The `snake_case` serde shape matches the database status values.
    #[test]
    fn task_state_serializes_snake_case() {
        let json = serde_json::to_string(&TaskState::InProgress).expect("serialize");
        assert_eq!(json, "\"in_progress\"");
        let parsed: TaskState = serde_json::from_str(&json).expect("parse");
        assert_eq!(parsed, TaskState::InProgress);
    }

    /// Every database value maps to exactly one state.
    #[test]
    fn task_state_roundtrips_through_db_strings() {
        for state in [
            TaskState::Pending,
            TaskState::InProgress,
            TaskState::Done,
            TaskState::Failed,
        ] {
            let from_db = TaskState::try_from(state.as_str()).expect("known state");
            assert_eq!(from_db, state);
        }
        assert!(TaskState::try_from("exploded").is_err());
    }
}
