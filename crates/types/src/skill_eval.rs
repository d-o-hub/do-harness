//! Read model for the `skill_evals` table: skill evaluation benchmarks.

use serde::{Deserialize, Serialize};

/// A persisted skill evaluation case.
///
/// Mirrors the `skill_evals` table and maps to the `evals/evals.json` case
/// schema (`id`, `prompt`, `expected_output`). One row per skill; the latest
/// evaluation overwrites it (upsert on the unique `skill_name`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SkillEval {
    /// Primary key.
    pub id: i64,
    /// Skill the evaluation belongs to.
    pub skill_name: String,
    /// The evaluation prompt.
    pub prompt: Option<String>,
    /// The expected outcome of the prompt.
    pub expected_outcome: Option<String>,
    /// Pass rate (fraction of graded answers that passed), 0.0 to 1.0.
    pub pass_rate: Option<f64>,
    /// Unix timestamp of creation.
    pub created_at: i64,
}
