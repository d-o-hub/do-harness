//! Which merge policy decides, and what it says.
//!
//! Two sources can decide readiness:
//!
//! * `plans/pr-merge-policy.toml` — the strict overlay this repository pins its
//!   own ruleset id in. When it exists, it decides exactly as it always has,
//!   including the drift gate.
//! * the server, via [`super::rules::MergeRules`] — the fallback an adopting
//!   repository gets, because it has no overlay and never will.
//!
//! The order is the whole design: an overlay is an explicit tightening, so its
//! presence must never be silently superseded. Absence is not an error, which
//! is what fixes the permanently-`UNKNOWN` merge authorization in any
//! repository without the file.

use std::path::Path;

use super::policy::POLICY_ABSENT_HINT;
use super::policy::{self, PolicyDecision};
use super::rules::MergeRules;

/// Which source produced the decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PolicySource {
    /// `plans/pr-merge-policy.toml` decided.
    Overlay,
    /// The server-derived rules decided.
    Derived,
}

/// The resolution of the merge policy for one pull request.
#[derive(Debug, Clone)]
pub struct ResolvedPolicy {
    /// Which source decided.
    pub source: PolicySource,
    /// Whether a policy decision could be reached at all.
    pub known: bool,
    /// Whether that decision permits merging.
    pub ok: bool,
    /// Digest pinned into the receipt: the overlay's sha256, or the derived
    /// rules' digest. A change to either invalidates a previously issued
    /// receipt, exactly like a moved head SHA.
    pub revision: Option<String>,
    /// The derived rules, when the derived source decided.
    pub rules: Option<MergeRules>,
    /// Non-fatal observations worth surfacing (for example a ruleset that
    /// requires no checks at all).
    pub warnings: Vec<String>,
}

impl ResolvedPolicy {
    fn unavailable() -> Self {
        Self {
            source: PolicySource::Derived,
            known: false,
            ok: false,
            revision: None,
            rules: None,
            warnings: Vec::new(),
        }
    }
}

/// Decides from the overlay when it exists, and from the server otherwise.
///
/// # Errors
///
/// Never returns an error for a missing overlay: that is the adopting-repository
/// case. API failures are reported inside [`ResolvedPolicy`] as `known = false`
/// so the caller keeps its existing fail-closed verdict.
pub fn resolve(root: &Path, base: &str) -> ResolvedPolicy {
    if root.join(policy::POLICY_PATH).exists() {
        // Overlay present: today's behaviour, unchanged, drift gate included.
        return match policy::evaluate(root) {
            Ok(report) => ResolvedPolicy {
                source: PolicySource::Overlay,
                known: report.decision != PolicyDecision::Unknown,
                ok: report.decision == PolicyDecision::Pass,
                revision: report.policy_sha256,
                rules: None,
                warnings: Vec::new(),
            },
            Err(err) => {
                // Carry the reason instead of dropping it: an overlay that
                // exists but cannot be evaluated is `policy_unavailable`, and a
                // report that says only "unavailable" leaves the reader with no
                // way to tell a permission problem from a malformed file.
                let mut resolved = ResolvedPolicy::unavailable();
                resolved
                    .warnings
                    .push(format!("the merge policy could not be evaluated: {err:#}"));
                resolved
            }
        };
    }

    match MergeRules::load(root, base) {
        Ok(rules) => {
            let mut warnings = Vec::new();
            if rules.has_no_required_checks() {
                warnings.push(format!(
                    "the server ruleset for '{base}' requires no status checks; \
                     an empty required set is not proof that verification ran \
                     ({POLICY_ABSENT_HINT})"
                ));
            }
            ResolvedPolicy {
                source: PolicySource::Derived,
                // A contradiction between the ruleset and the repository's own
                // settings is a known blocker, not an unknown: the server said
                // something unambiguous and it cannot be honoured.
                known: true,
                ok: !rules.merge_method_unavailable(),
                revision: Some(rules.digest.clone()),
                rules: Some(rules),
                warnings,
            }
        }
        Err(err) => {
            let mut resolved = ResolvedPolicy::unavailable();
            resolved
                .warnings
                .push(format!("the server ruleset could not be read: {err:#}"));
            resolved
        }
    }
}
