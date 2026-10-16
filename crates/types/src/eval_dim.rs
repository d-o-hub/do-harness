//! Evaluation dimension contract for skill evals.
//!
//! The five dimensions mirror the NVIDIA `SkillEvaluator` scoring model so
//! per-dimension Skill Lift is comparable: a skill can be correct but
//! undiscoverable, or effective but expensive, and the aggregate pass rate
//! alone cannot tell those apart.

use serde::{Deserialize, Serialize};
use std::str::FromStr;
use thiserror::Error;

/// Wire names [`EvalDim::from_str`] accepts.
const EXPECTED_EVAL_DIMS: &str = "correctness|discoverability|effectiveness|efficiency|security";

/// Error returned when a stored or supplied evaluation dimension is not one
/// of the five scored dimensions.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("unknown eval dimension '{value}': expected one of {EXPECTED_EVAL_DIMS}")]
pub struct EvalDimParseError {
    /// The unrecognized value.
    pub value: String,
}

/// One of the five scored evaluation dimensions.
///
/// The wire name ([`EvalDim::as_str`], `Display`, `FromStr`, and the serde
/// `rename_all = "lowercase"`) is one vocabulary shared by fixtures, the
/// database, and reports, so a fixture written with a different spelling would
/// otherwise score as a different dimension.
///
/// ```
/// # use std::str::FromStr;
/// # use do_harness_types::EvalDim;
/// let dim = EvalDim::from_str("efficiency").expect("wire name");
/// assert_eq!(dim, EvalDim::Efficiency);
/// assert_eq!(dim.as_str(), "efficiency");
/// assert_eq!(dim.to_string(), "efficiency");
/// assert_eq!(EvalDim::all().len(), 5);
/// assert!(EvalDim::from_str("performance").is_err());
/// ```
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

impl FromStr for EvalDim {
    type Err = EvalDimParseError;

    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        match raw {
            "correctness" => Ok(EvalDim::Correctness),
            "discoverability" => Ok(EvalDim::Discoverability),
            "effectiveness" => Ok(EvalDim::Effectiveness),
            "efficiency" => Ok(EvalDim::Efficiency),
            "security" => Ok(EvalDim::Security),
            _ => Err(EvalDimParseError {
                value: raw.to_owned(),
            }),
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
            assert_eq!(dim.to_string(), dim.as_str());
        }
        assert!(serde_json::from_str::<EvalDim>("\"speed\"").is_err());
        let err = "speed".parse::<EvalDim>().expect_err("unknown dimension");
        assert_eq!(err.value, "speed");
        assert!(err.to_string().contains(EXPECTED_EVAL_DIMS));
    }
}
