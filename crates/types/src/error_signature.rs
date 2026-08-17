//! Read model for the `error_signatures` table: fail-fast accounting.

use serde::{Deserialize, Serialize};

/// A recurring error signature tracked for the fail-fast policy.
///
/// Mirrors the `error_signatures` table; `signature` is unique so consecutive
/// failures increment [`attempt_count`](Self::attempt_count).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ErrorSignature {
    /// Primary key.
    pub id: i64,
    /// Stable error signature (e.g. `sensor:clippy` or a compiler fingerprint).
    pub signature: String,
    /// Owning task id, when the signature belongs to a task.
    pub task_id: Option<i64>,
    /// Number of consecutive recorded attempts.
    pub attempt_count: i64,
    /// Optional diagnostic message (truncated output).
    pub message: Option<String>,
    /// Unix timestamp of the first recording.
    pub created_at: i64,
}
