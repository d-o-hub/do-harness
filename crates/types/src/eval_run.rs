//! Types for skill-eval run history and grader tamper-evidence.

use serde::{Deserialize, Serialize};

/// One recorded execution of a skill's evaluation suite.
///
/// Unlike the collapsed latest-row [`crate::skill_eval::SkillEval`] read
/// model, runs are append-only so improvement across rounds is measurable.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SkillEvalRun {
    /// Database id of the run.
    pub id: i64,
    /// Skill the evaluation belongs to.
    pub skill_name: String,
    /// Number of graded assertions in the run.
    pub graded: i64,
    /// Number of graded assertions that passed.
    pub passed: i64,
    /// Fraction of graded assertions that passed; `None` when the skill has
    /// no graded assertions.
    pub pass_rate: Option<f64>,
    /// Unix timestamp when the run was recorded.
    pub ran_at: i64,
}

/// Tamper-evidence baseline for a skill's graders.
///
/// Recorded when a human blesses a fully green eval: drift between these
/// hashes and the on-disk grader files fails subsequent evals until reviewed
/// and re-blessed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GraderBaseline {
    /// Skill whose graders are baselined.
    pub skill_name: String,
    /// SHA-256 (hex) of `evals/walkthrough.sh` at bless time.
    pub walkthrough_sha: String,
    /// SHA-256 (hex) of `evals/evals.json` at bless time.
    pub specs_sha: String,
    /// Unix timestamp of the bless.
    pub blessed_at: i64,
}
