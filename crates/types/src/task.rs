//! Read model for the `tasks` table: HTN task instances.

use serde::{Deserialize, Serialize};

use crate::htn::TaskState;

/// A persisted HTN task instance.
///
/// Mirrors the `tasks` table; the method catalog lives in [`crate::htn`]
/// while this is a single execution record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskRecord {
    /// Primary key.
    pub id: i64,
    /// Parent task id, when this task is a subtask.
    pub parent_id: Option<i64>,
    /// Human-readable task title.
    pub title: String,
    /// Name of the HTN method this task follows (e.g. `vertical-event-slice`).
    pub method: Option<String>,
    /// Index of the current subtask within the method.
    pub subtask_index: i64,
    /// Lifecycle state, stored as `snake_case`.
    pub status: TaskState,
    /// Recorded precondition guard the task was created under.
    pub precondition: Option<String>,
    /// Unix timestamp of creation.
    pub created_at: i64,
    /// Unix timestamp of the last status or index change.
    pub updated_at: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Unknown fields are rejected so stale payloads fail loudly.
    #[test]
    fn task_record_rejects_unknown_fields() {
        let json = r#"{"id": 1, "parent_id": null, "title": "t", "method": null,
            "subtask_index": 0, "status": "pending", "precondition": null,
            "created_at": 1, "updated_at": 1, "bogus": true}"#;
        assert!(serde_json::from_str::<TaskRecord>(json).is_err());
    }
}
