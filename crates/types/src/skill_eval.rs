//! Read model for the `skill_evals` table: skill evaluation benchmarks.

use serde::{Deserialize, Serialize};

/// A persisted skill evaluation case.
///
/// Mirrors the `skill_evals` table and maps to the `evals/evals.json` case
/// schema (`id`, `prompt`, `expected_output`); `token_efficiency` stays
/// `None` until a live run measures it.
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
    /// Token efficiency measured by a live run, when available.
    pub token_efficiency: Option<f64>,
    /// Pass rate (fraction of structurally valid cases), 0.0 to 1.0.
    pub pass_rate: Option<f64>,
    /// Unix timestamp of creation.
    pub created_at: i64,
}
