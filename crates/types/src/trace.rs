//! Read model for the `traces` table: interaction/execution traces.

use serde::{Deserialize, Serialize};

/// A single interaction or execution trace for distillation.
///
/// Mirrors the `traces` table: the command that ran, the error diff it
/// produced, and the steps taken to resolve it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Trace {
    /// Primary key.
    pub id: i64,
    /// Owning task id, when the trace belongs to a task.
    pub task_id: Option<i64>,
    /// Session identifier grouping related traces.
    pub session_id: String,
    /// The command that was executed.
    pub command: Option<String>,
    /// Error diff or failure output captured.
    pub error_diff: Option<String>,
    /// Steps taken to resolve the failure.
    pub resolution_steps: Option<String>,
    /// Unix timestamp of creation.
    pub created_at: i64,
}
