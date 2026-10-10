//! Proof skipping: which residual units a trusted policy may mark proven.
//!
//! Rules come only from `.github/pr-gate.toml` at the merge base. A unit is
//! proven when its path matches a `[proof] mechanical` glob and no
//! `behavioral` glob, or when the change is structural (rename-only or
//! mode-only). Behavioral matches always stay residual; a mechanical claim
//! contradicted by a behavioral rule is revoked and recorded as
//! `false_proven`. Absent, malformed, or partially invalid policy proves
//! nothing.

use globset::{Glob, GlobSet, GlobSetBuilder};
use serde::{Deserialize, Serialize};

use super::diff::{HEADER_MODE_CHANGE, HEADER_RENAME_ONLY, Unit};

/// Gate policy path, read from the merge-base revision only.
pub const POLICY_PATH: &str = ".github/pr-gate.toml";

/// Paths that are privileged and can never be proven, whatever the rules say.
const PROTECTED_PATHS: &[&str] = &[POLICY_PATH];

/// Policy-declared proof rules from the `[proof]` table.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProofRules {
    /// Globs whose changed units are mechanical and may be skipped.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mechanical: Vec<String>,
    /// Globs that are never proven; they override `mechanical`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub behavioral: Vec<String>,
}

/// Parsed `.github/pr-gate.toml`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GatePolicy {
    /// Proof skipping configuration.
    #[serde(default)]
    pub proof: ProofRules,
}

/// Outcome of evaluating one unit against the proof rules.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    /// No trusted proof applies; the unit stays residual.
    Residual,
    /// The unit is proven and belongs in the audit list.
    Proven,
    /// A mechanical claim was revoked; the unit stays residual with a reason.
    Revoked {
        /// Why the proof claim is untrusted.
        reason: String,
    },
}

/// Compiled proof matcher.
pub struct Matcher {
    mechanical: Option<GlobSet>,
    behavioral: Option<GlobSet>,
    active: bool,
    warnings: Vec<String>,
}

impl Matcher {
    /// Compile `rules`; `None` (absent or malformed policy) proves nothing.
    #[must_use]
    pub fn compile(rules: Option<&ProofRules>) -> Self {
        let Some(rules) = rules else {
            return Self::inactive();
        };
        let mut warnings = Vec::new();
        let mechanical = build(&rules.mechanical, "mechanical", &mut warnings);
        let behavioral = build(&rules.behavioral, "behavioral", &mut warnings);
        let active = warnings.is_empty();
        Self {
            mechanical: active.then_some(mechanical).flatten(),
            behavioral: active.then_some(behavioral).flatten(),
            active,
            warnings,
        }
    }

    /// Diagnostics from rule compilation; any warning disables all proofs.
    #[must_use]
    pub fn warnings(&self) -> &[String] {
        &self.warnings
    }

    /// Evaluates one unit against the compiled rules.
    #[must_use]
    pub fn evaluate(&self, unit: &Unit) -> Verdict {
        if !self.active {
            return Verdict::Residual;
        }
        let mechanical = matches(self.mechanical.as_ref(), &unit.path);
        if matches(self.behavioral.as_ref(), &unit.path)
            || PROTECTED_PATHS.contains(&unit.path.as_str())
        {
            if mechanical {
                return Verdict::Revoked {
                    reason: format!(
                        "{} matches both mechanical and behavioral proof rules",
                        unit.path
                    ),
                };
            }
            return Verdict::Residual;
        }
        if mechanical || is_structural(unit) {
            Verdict::Proven
        } else {
            Verdict::Residual
        }
    }

    fn inactive() -> Self {
        Self {
            mechanical: None,
            behavioral: None,
            active: false,
            warnings: Vec::new(),
        }
    }
}

/// Whether the unit is a content-free structural change.
fn is_structural(unit: &Unit) -> bool {
    unit.lines.is_empty()
        && matches!(
            unit.header.as_deref(),
            Some(HEADER_RENAME_ONLY | HEADER_MODE_CHANGE)
        )
}

/// Compiles one glob list; an invalid pattern disables all proofs.
fn build(patterns: &[String], label: &str, warnings: &mut Vec<String>) -> Option<GlobSet> {
    if patterns.is_empty() {
        return None;
    }
    let mut builder = GlobSetBuilder::new();
    for pattern in patterns {
        match Glob::new(pattern) {
            Ok(glob) => {
                builder.add(glob);
            }
            Err(err) => {
                warnings.push(format!(
                    "invalid {label} glob in {POLICY_PATH}: {pattern}: {err}"
                ));
                return None;
            }
        }
    }
    match builder.build() {
        Ok(set) => Some(set),
        Err(err) => {
            warnings.push(format!("invalid {label} proof rules: {err}"));
            None
        }
    }
}

/// Whether `path` matches the compiled set.
fn matches(set: Option<&GlobSet>, path: &str) -> bool {
    set.is_some_and(|set| set.is_match(path))
}
