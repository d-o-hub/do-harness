//! Proof skipping: which residual units a trusted policy may mark exempt or proven.
//!
//! Rules come only from `.github/pr-gate.toml` at the merge base. A unit is
//! exempt when its paths match a `[proof] mechanical` glob and no
//! `behavioral` or protected glob. Behavioral matches always stay residual; a
//! mechanical claim contradicted by a behavioral or protected rule is revoked
//! and recorded as `false_proven`. Absent, malformed, or partially invalid
//! policy proves/exempts nothing.

use globset::{Glob, GlobSet, GlobSetBuilder};
use serde::{Deserialize, Serialize};

use super::diff::Unit;

/// Gate policy path, read from the merge-base revision only.
pub const POLICY_PATH: &str = ".github/pr-gate.toml";

/// Paths that are privileged and can never be proven or exempted, whatever the rules say.
const PROTECTED_PATHS: &[&str] = &[POLICY_PATH];

/// Policy-declared proof rules from the `[proof]` table.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProofRules {
    /// Globs whose changed units are mechanical and may be exempted.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mechanical: Vec<String>,
    /// Globs that are never exempted; they override `mechanical`.
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
    /// No trusted proof or exemption applies; the unit stays residual.
    Residual,
    /// Policy exempts the unit from review.
    Exempt,
    /// Deterministic evidence proved behavior preservation.
    #[allow(dead_code)]
    Proven,
    /// A mechanical claim was revoked; the unit stays residual with a reason.
    Revoked {
        /// Why the proof/exemption claim is untrusted.
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
        let mut paths = vec![unit.path.as_str()];
        if let Some(old) = unit.old_path.as_deref() {
            if old != unit.path.as_str() {
                paths.push(old);
            }
        }
        let is_protected = paths.iter().any(|p| PROTECTED_PATHS.contains(p));
        let is_behavioral = matches_any(self.behavioral.as_ref(), &paths);
        let mechanical_match_all = matches_all(self.mechanical.as_ref(), &paths);
        let mechanical_match_any = matches_any(self.mechanical.as_ref(), &paths);

        if is_protected || is_behavioral {
            if mechanical_match_any {
                return Verdict::Revoked {
                    reason: format!(
                        "{} matches both mechanical and behavioral or protected proof rules",
                        unit.path
                    ),
                };
            }
            return Verdict::Residual;
        }

        if mechanical_match_all {
            Verdict::Exempt
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

/// Whether any path matches the compiled set.
fn matches_any(set: Option<&GlobSet>, paths: &[&str]) -> bool {
    let Some(set) = set else { return false; };
    paths.iter().any(|p| set.is_match(p))
}

/// Whether all paths match the compiled set.
fn matches_all(set: Option<&GlobSet>, paths: &[&str]) -> bool {
    let Some(set) = set else { return false; };
    !paths.is_empty() && paths.iter().all(|p| set.is_match(p))
}
