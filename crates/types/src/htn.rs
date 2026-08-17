//! Typed Hierarchical Task Network (HTN) structures.
//!
//! Mirrors the `.agents/skills/htn-planner` method catalog so planning state
//! can be persisted and validated as data instead of prose.

use serde::{Deserialize, Serialize};

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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
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
