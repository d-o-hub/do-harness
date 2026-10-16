//! Evaluation dimension contract for skill evals.
//!
//! The five dimensions mirror the NVIDIA `SkillEvaluator` scoring model so
//! per-dimension Skill Lift is comparable: a skill can be correct but
//! undiscoverable, or effective but expensive, and the aggregate pass rate
//! alone cannot tell those apart.

use serde::{Deserialize, Serialize};

/// One of the five scored evaluation dimensions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[serde(deny_unknown_fields)]
pub enum EvalDim {
    /// Is the final answer correct?
    Correctness,
    /// Does the skill load when relevant and stay unloaded when not?
    Discoverability,
    /// Did the agent reach the goal and follow the expected workflow?
    Effectiveness,
    /// Were there wasted steps or redundant tool calls?
    Efficiency,
    /// Did the run avoid unsafe operations, secret leakage, and
    /// unauthorized access?
    Security,
}

impl EvalDim {
    /// All dimensions in canonical display order.
    #[must_use]
    pub const fn all() -> [EvalDim; 5] {
        [
            EvalDim::Correctness,
            EvalDim::Discoverability,
            EvalDim::Effectiveness,
            EvalDim::Efficiency,
            EvalDim::Security,
        ]
    }

    /// Lowercase wire name shared by fixtures, the database, and reports.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            EvalDim::Correctness => "correctness",
            EvalDim::Discoverability => "discoverability",
            EvalDim::Effectiveness => "effectiveness",
            EvalDim::Efficiency => "efficiency",
            EvalDim::Security => "security",
        }
    }
}

impl Default for EvalDim {
    /// Untagged fixture cases score as workflow effectiveness, preserving
    /// the pre-dimension meaning of the aggregate pass rate.
    fn default() -> Self {
        EvalDim::Effectiveness
    }
}

impl std::fmt::Display for EvalDim {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::str::FromStr for EvalDim {
    type Err = String;

    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        match raw {
            "correctness" => Ok(EvalDim::Correctness),
            "discoverability" => Ok(EvalDim::Discoverability),
            "effectiveness" => Ok(EvalDim::Effectiveness),
            "efficiency" => Ok(EvalDim::Efficiency),
            "security" => Ok(EvalDim::Security),
            _ => Err(format!(
                "unknown eval dimension '{raw}': expected one of \
                 correctness|discoverability|effectiveness|efficiency|security"
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    /// Fixtures, the database, and reports all quote [`EvalDim::as_str`], so the
    /// serde name and the `FromStr` name must be the same vocabulary: a serde
    /// rename alone would desynchronise the three.
    #[test]
    fn serde_names_match_the_stored_names() {
        for dim in EvalDim::all() {
            let json = serde_json::to_string(&dim).expect("serialize");
            assert_eq!(json, format!("\"{dim}\""));
            let parsed: EvalDim = serde_json::from_str(&json).expect("parse");
            assert_eq!(parsed, dim);
            assert_eq!(dim.as_str().parse::<EvalDim>().expect("from_str"), dim);
        }
        assert!(serde_json::from_str::<EvalDim>("\"speed\"").is_err());
    }
}
