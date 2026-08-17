//! Read model for the `beats` table: execution heartbeat/step tracking.

use serde::{Deserialize, Serialize};

/// A single execution beat (heartbeat or step) recorded for a task.
///
/// Mirrors the `beats` table; `status` and `beat_type` are free-form labels
/// (e.g. `sensor` / `ok`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Beat {
    /// Primary key.
    pub id: i64,
    /// Owning task id, when the beat belongs to a task.
    pub task_id: Option<i64>,
    /// Beat kind (e.g. `sensor`, `step`).
    pub beat_type: String,
    /// Outcome label (e.g. `ok`, `failed`).
    pub status: String,
    /// Exit code of the sensor that produced this beat.
    pub sensor_exit_code: Option<i32>,
    /// Unix timestamp when the beat started.
    pub started_at: i64,
    /// Unix timestamp when the beat completed, when known.
    pub completed_at: Option<i64>,
}
