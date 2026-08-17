//! Read model for the `heuristics` table: distilled learnings.

use serde::{Deserialize, Serialize};

/// A distilled heuristic attached to a skill.
///
/// Mirrors the `heuristics` table: the generalized pattern and its source
/// trace, so distillation stays auditable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Heuristic {
    /// Primary key.
    pub id: i64,
    /// Skill the heuristic belongs to.
    pub skill_name: String,
    /// Generalized pattern, stripped of project-specific identifiers.
    pub pattern: String,
    /// Optional description of when the pattern applies.
    pub description: Option<String>,
    /// Source trace the heuristic was distilled from.
    pub source_trace_id: Option<i64>,
    /// Unix timestamp of creation.
    pub created_at: i64,
}
