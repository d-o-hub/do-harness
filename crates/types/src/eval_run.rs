//! Types for skill-eval run history and grader tamper-evidence.

use serde::{Deserialize, Serialize};

/// How a skill-eval run was executed.
///
/// Lift means different things per mode: deterministic runs measure artifact
/// dependence on the walkthrough residue, agent runs measure guidance value
/// with a real agent in the loop. Persisting the mode keeps the two honest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EvalMode {
    /// Deterministic `evals/walkthrough.sh` residue (default).
    #[default]
    Deterministic,
    /// External agent command per eval case (`eval --agent-cmd`).
    Agent,
}

impl EvalMode {
    /// Lowercase wire name shared by the database and reports.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            EvalMode::Deterministic => "deterministic",
            EvalMode::Agent => "agent",
        }
    }
}

impl std::fmt::Display for EvalMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::str::FromStr for EvalMode {
    type Err = String;

    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        match raw {
            "deterministic" => Ok(EvalMode::Deterministic),
            "agent" => Ok(EvalMode::Agent),
            _ => Err(format!(
                "unknown eval mode '{raw}': expected deterministic|agent"
            )),
        }
    }
}

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
    /// How the run executed (deterministic walkthrough or agent command).
    pub mode: EvalMode,
    /// Number of graded assertions in the run.
    pub graded: i64,
    /// Number of graded assertions that passed.
    pub passed: i64,
    /// Fraction of graded assertions that passed; `None` when the skill has
    /// no graded assertions.
    pub pass_rate: Option<f64>,
    /// Fraction that passed in the without-skill baseline run (guidance
    /// stripped); `None` when lift was not measured (`--no-lift` or runs
    /// recorded before lift existed). Skill Lift is `pass_rate` minus this.
    pub without_pass_rate: Option<f64>,
    /// Context-cost proxy: words in the installed `SKILL.md` plus
    /// `references/`. Tracks whether a skill improves results by hogging
    /// the context window.
    pub skill_words: Option<i64>,
    /// Execution-cost proxy: walkthrough or agent wall time in seconds.
    pub walk_secs: Option<f64>,
    /// Unix timestamp when the run was recorded.
    pub ran_at: i64,
}

/// Per-dimension breakdown of one [`SkillEvalRun`].
///
/// One row per dimension that had at least one graded assertion; dimensions
/// with no coverage are absent rather than zero so "unevaluated" stays
/// distinguishable from "failed".
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SkillEvalDimRate {
    /// Database id of the parent run.
    pub run_id: i64,
    /// Dimension wire name (`correctness`, `discoverability`, ...).
    pub dim: String,
    /// Graded assertions in this dimension (with-skill run).
    pub graded: i64,
    /// Passing assertions in this dimension (with-skill run).
    pub passed: i64,
    /// Passing assertions in the without-skill baseline run, when measured.
    pub without_passed: Option<i64>,
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

/// One append-only entry in a skill's bless history.
///
/// The `grader_baselines` row is latest-wins; this history preserves who
/// approved each hash change and when, so a compromised or mistaken baseline
/// can be audited after the fact.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SkillEvalBless {
    /// Database id of the bless entry.
    pub id: i64,
    /// Skill whose graders were blessed.
    pub skill_name: String,
    /// SHA-256 (hex) of `evals/walkthrough.sh` at bless time.
    pub walkthrough_sha: String,
    /// SHA-256 (hex) of `evals/evals.json` at bless time.
    pub specs_sha: String,
    /// Identity that approved the bless.
    pub approver: String,
    /// Optional reason recorded with the bless.
    pub reason: Option<String>,
    /// Unix timestamp of the bless.
    pub blessed_at: i64,
}
