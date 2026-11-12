//! The auditable CI policy that decides which check contexts gate a merge.
//!
//! Split out of `checks.rs` so the classifier stays about classification and
//! the loader stays about reading files. Two inputs are honoured here:
//!
//! * `.git/pr-triage/checks-policy.toml` — `allow_no_ci`, `required`, and
//!   `allow_skips`;
//! * `.git/pr-triage/checks-allow.txt` — the legacy one-name-per-line skip
//!   allowlist, still merged into the same set.
//!
//! A missing policy is the strict default. A malformed one is an error, never a
//! silent fallback to permissive defaults.

use std::collections::BTreeSet;
use std::path::Path;

use anyhow::{Context, Result};
use serde::Deserialize;

/// Directory holding the auditable CI policy, relative to the repository root.
pub const POLICY_DIR: &str = ".git/pr-triage";
/// TOML policy file: `allow_no_ci`, `required`, and `allow_skips`.
pub const POLICY_FILE: &str = "checks-policy.toml";
/// Legacy one-name-per-line skip allowlist, still honoured.
pub const ALLOW_FILE: &str = "checks-allow.txt";

/// Loaded policy, with the skip allowlist as a set.
#[derive(Debug, Clone, Default)]
pub struct ChecksPolicy {
    /// An empty check set is a pass.
    pub allow_no_ci: bool,
    /// Check names that must be present and passing.
    pub required: Vec<String>,
    /// Check names whose skipped/neutral conclusion is acceptable.
    pub allow_skips: BTreeSet<String>,
    /// Whether `required` is a decision rather than an absence.
    ///
    /// An empty `required` list means two very different things: an operator
    /// declared that nothing gates, or the tool never found out. Only the first
    /// may let an optional skip pass, so the distinction is carried explicitly
    /// instead of inferred from emptiness — otherwise an unreadable ruleset
    /// would silently widen what merges.
    pub required_known: bool,
}

impl ChecksPolicy {
    /// Whether `name` gates a merge under this policy.
    ///
    /// A required context is blocking whatever its conclusion, which is the
    /// whole point of the server naming it. Everything else is optional CI:
    /// useful to report, never a reason to refuse a merge.
    #[must_use]
    pub fn is_required(&self, name: &str) -> bool {
        self.required.iter().any(|required| required == name)
    }

    /// `allow_skips` entries that name a *required* context.
    ///
    /// The two lists contradict each other: one says "this context must pass",
    /// the other says "its skip is fine". A required context can never be
    /// satisfied by a skip, so the entry is inert, and accepting it silently
    /// would let a reviewer believe a gate had been relaxed when it had not.
    #[must_use]
    pub fn contradictory_skips(&self) -> Vec<String> {
        self.allow_skips
            .iter()
            .filter(|name| self.is_required(name))
            .cloned()
            .collect()
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PolicyFile {
    #[serde(default)]
    allow_no_ci: bool,
    /// Absent means "the operator did not declare a required set", which is not
    /// the same as declaring an empty one: the server's set still applies. A
    /// `Vec` with `serde(default)` would erase that difference, and a policy
    /// written only to add an `allow_skips` entry would silently drop every
    /// server-required context — widening what merges, the one direction a
    /// tightening knob must never move.
    #[serde(default)]
    required: Option<Vec<String>>,
    #[serde(default)]
    allow_skips: Vec<String>,
}

/// Fills `required` from the resolved merge policy.
///
/// This is where "which context gates a merge" comes from when the local policy
/// does not declare it: the same resolution `pr ready` authorizes against, so
/// the CI verdict and the merge decision cannot disagree about what is required.
///
/// The local declaration wins when there is one — it is the auditable tightening
/// knob, and a repository that names its contexts in
/// `.git/pr-triage/checks-policy.toml` already satisfies the server, which is
/// what `pr policy` drift-checks. `resolved.required` of `None` means the source
/// could not be read, so nothing is filled and `required_known` stays false: the
/// classifier then refuses to treat a skip as a pass, because "nothing requires
/// this context" was never established.
pub fn with_resolved_required(
    mut policy: ChecksPolicy,
    resolved: &super::policy_source::ResolvedPolicy,
) -> ChecksPolicy {
    if policy.required_known {
        return policy;
    }
    if let Some(required) = &resolved.required {
        policy.required.clone_from(required);
        policy.required_known = true;
    }
    policy
}

/// Reads the repository's CI policy.
///
/// A missing policy is the strict default: CI is expected, no check is
/// required by name, and no skip is allowed. A malformed policy is an error,
/// never a silent fallback to permissive defaults.
///
/// # Errors
///
/// Returns an error when a policy file exists but cannot be read or parsed.
pub fn load_policy(root: &Path) -> Result<ChecksPolicy> {
    let dir = root.join(POLICY_DIR);
    let mut policy = ChecksPolicy::default();

    let path = dir.join(POLICY_FILE);
    if path.is_file() {
        let text = std::fs::read_to_string(&path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        let file: PolicyFile = toml::from_str(&text)
            .with_context(|| format!("invalid CI policy {}", path.display()))?;
        policy.allow_no_ci = file.allow_no_ci;
        if let Some(required) = file.required {
            policy.required = required;
            policy.required_known = true;
        }
        policy.allow_skips.extend(file.allow_skips);
    }

    let allow = dir.join(ALLOW_FILE);
    if allow.is_file() {
        let text = std::fs::read_to_string(&allow)
            .with_context(|| format!("failed to read {}", allow.display()))?;
        for line in text.lines() {
            let line = line.trim();
            if !line.is_empty() && !line.starts_with('#') {
                policy.allow_skips.insert(line.to_owned());
            }
        }
    }

    Ok(policy)
}
