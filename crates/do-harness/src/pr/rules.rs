//! Merge rules derived from the server, for repositories with no local policy.
//!
//! `plans/pr-merge-policy.toml` is a strict overlay this repository writes to
//! pin its own ruleset by id. An adopting repository has no such file, so
//! reading only the overlay left merge authorization permanently `UNKNOWN` and
//! made the tool's "works in any repository" claim false for the merge path.
//!
//! The authoritative rules already exist server-side and are readable through
//! the API, so this module reads them and the overlay becomes optional:
//!
//! * overlay present -> [`super::policy::evaluate`] decides, exactly as before;
//! * overlay absent  -> the rules derived here decide;
//! * rules unreadable -> the caller keeps the existing fail-closed behaviour.
//!
//! Required-check identity is the tuple `(context, integration_id)`, not the
//! context alone. A gate that matches on name only lets any app publish a check
//! called `CI / Required` and satisfy it, so the comparison is against the
//! check run's owning `app.id` and fails closed when a run carries none.

use std::path::Path;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// How the server permits a pull request to be merged.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MergeMethod {
    /// A merge commit.
    Merge,
    /// One squashed commit.
    Squash,
    /// A linear replay of the commits.
    Rebase,
}

impl MergeMethod {
    /// Wire name, matching the ruleset's `allowed_merge_methods` values.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Merge => "merge",
            Self::Squash => "squash",
            Self::Rebase => "rebase",
        }
    }

    fn parse(name: &str) -> Option<Self> {
        match name {
            "merge" => Some(Self::Merge),
            "squash" => Some(Self::Squash),
            "rebase" => Some(Self::Rebase),
            _ => None,
        }
    }
}

/// One required status check, identified by the app that may publish it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RequiredContext {
    /// The check context (name) the ruleset requires.
    pub context: String,
    /// The app id allowed to satisfy it, when the ruleset names one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub integration_id: Option<u64>,
}

/// The merge rules a base branch's server-side configuration imposes.
///
/// The independent flags are the ruleset's own axes (`strict_up_to_date`,
/// `linear_history`, `merge_queue`, `thread_resolution`) plus the digest; they
/// are deliberately separate because each answers a different merge question,
/// so the struct is a faithful model rather than a bundle of related booleans.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[allow(clippy::struct_excessive_bools)]
pub struct MergeRules {
    /// Required status checks, with the app identity that may satisfy each.
    pub required: Vec<RequiredContext>,
    /// Whether the head must be up to date with the base before merging.
    pub strict_up_to_date: bool,
    /// Merge methods the ruleset permits, in a stable order.
    pub allowed_methods: Vec<MergeMethod>,
    /// Whether a linear history is required.
    pub linear_history: bool,
    /// Whether a merge queue is configured.
    pub merge_queue: bool,
    /// Whether every review conversation must be resolved.
    pub thread_resolution: bool,
    /// Merge methods the repository itself has enabled.
    ///
    /// Kept separately from [`Self::allowed_methods`] so a ruleset that permits
    /// only a method the repository disables is detectable as a contradiction
    /// instead of silently narrowing to "nothing allowed".
    pub repository_methods: Option<Vec<MergeMethod>>,
    /// Digest over the normalised rules, pinned into the merge receipt.
    pub digest: String,
}

/// One way the head's checks fail the derived ruleset.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuleViolation {
    /// Stable machine-readable code.
    pub code: &'static str,
    /// The required context involved.
    pub context: String,
    /// Human-readable explanation.
    pub detail: String,
}

/// Stable codes for [`RuleViolation`], mirrored as authorization reason codes.
pub mod codes {
    /// The required context has no check run at all.
    pub const REQUIRED_CHECK_MISSING: &str = "required_check_missing";
    /// A run publishes the context but is not passing.
    pub const REQUIRED_CHECK_FAILING: &str = "required_check_failing";
    /// A passing run publishes the context from an app the ruleset did not name.
    pub const REQUIRED_CHECK_APP_MISMATCH: &str = "required_check_app_mismatch";
}

/// Raw rule as returned by `GET /repos/{owner}/{repo}/rules/branches/{base}`.
#[derive(Debug, Deserialize)]
pub(crate) struct RawRule {
    #[serde(rename = "type")]
    rule_type: String,
    #[serde(default)]
    parameters: Option<serde_json::Value>,
}

/// Repository settings that bound which merge methods can actually be used.
///
/// Each field is `Option` rather than `bool` on purpose. `GitHub` returns these
/// only to a caller with admin permission on the repository, and a missing field
/// means "not reported", never "disabled": defaulting to `false` turns an
/// unreadable setting into a confident contradiction, which is a wrong blocker
/// rather than an honest unknown.
#[derive(Debug, Deserialize)]
#[allow(clippy::struct_field_names)]
pub(crate) struct RawRepo {
    #[serde(default)]
    pub(crate) allow_merge_commit: Option<bool>,
    #[serde(default)]
    pub(crate) allow_squash_merge: Option<bool>,
    #[serde(default)]
    pub(crate) allow_rebase_merge: Option<bool>,
}

impl RawRepo {
    /// The merge methods the repository reports as enabled, or `None` when the
    /// response establishes none of them.
    fn reported_methods(&self) -> Option<Vec<MergeMethod>> {
        let reported = [
            (MergeMethod::Merge, self.allow_merge_commit),
            (MergeMethod::Squash, self.allow_squash_merge),
            (MergeMethod::Rebase, self.allow_rebase_merge),
        ];
        if reported.iter().all(|(_, enabled)| enabled.is_none()) {
            return None;
        }
        Some(
            reported
                .into_iter()
                .filter_map(|(method, enabled)| (enabled == Some(true)).then_some(method))
                .collect(),
        )
    }
}

impl MergeRules {
    /// Reads the base branch's ruleset and the repository's merge settings.
    ///
    /// # Errors
    ///
    /// Returns an error when either API call fails or returns a payload the
    /// models cannot read. The caller treats that as `policy_unavailable` and
    /// stays fail-closed; it never falls back to a permissive default.
    pub fn load(root: &Path, base: &str) -> Result<Self> {
        let rules_endpoint = format!("repos/{{owner}}/{{repo}}/rules/branches/{base}");
        let rules: Vec<RawRule> =
            crate::gh::json(root, &["api", &rules_endpoint], "api branch rules")
                .context("failed to read the branch ruleset")?;
        let repo: RawRepo = crate::gh::json(root, &["api", "repos/{owner}/{repo}"], "api repo")
            .context("failed to read repository merge settings")?;
        Ok(Self::from_payloads(&rules, &repo))
    }

    /// Builds the rules from an already-fetched ruleset and repository payload.
    ///
    /// Split from [`Self::load`] so tests can drive a fixture without a network
    /// or a `gh` binary.
    #[must_use]
    pub fn from_payloads(rules: &[RawRule], repo: &RawRepo) -> Self {
        let mut required = Vec::new();
        let mut strict_up_to_date = false;
        let mut methods: Vec<MergeMethod> = Vec::new();
        let mut linear_history = false;
        let mut merge_queue = false;
        let mut thread_resolution = false;

        for rule in rules {
            match rule.rule_type.as_str() {
                "required_status_checks" => {
                    let Some(params) = &rule.parameters else {
                        continue;
                    };
                    strict_up_to_date = params
                        .get("strict_required_status_checks_policy")
                        .and_then(serde_json::Value::as_bool)
                        .unwrap_or(false);
                    if let Some(checks) = params
                        .get("required_status_checks")
                        .and_then(serde_json::Value::as_array)
                    {
                        for check in checks {
                            let Some(context) = check.get("context").and_then(|v| v.as_str())
                            else {
                                continue;
                            };
                            required.push(RequiredContext {
                                context: context.to_owned(),
                                integration_id: check
                                    .get("integration_id")
                                    .and_then(serde_json::Value::as_u64),
                            });
                        }
                    }
                }
                "pull_request" => {
                    let Some(params) = &rule.parameters else {
                        continue;
                    };
                    thread_resolution = params
                        .get("required_review_thread_resolution")
                        .and_then(serde_json::Value::as_bool)
                        .unwrap_or(false);
                    if let Some(list) = params
                        .get("allowed_merge_methods")
                        .and_then(serde_json::Value::as_array)
                    {
                        for name in list.iter().filter_map(serde_json::Value::as_str) {
                            if let Some(method) = MergeMethod::parse(name) {
                                methods.push(method);
                            }
                        }
                    }
                }
                "required_linear_history" => linear_history = true,
                "merge_queue" => merge_queue = true,
                _ => {}
            }
        }

        // A ruleset may omit `allowed_merge_methods`, in which case the
        // repository's own settings are the only constraint. When those are
        // not reported either, there is nothing to bound the set by: an empty
        // list here means "not reported", and `merge_method_unavailable` must
        // read it as unknown rather than as a contradiction.
        let repository_methods = repo.reported_methods();
        if methods.is_empty() {
            methods = repository_methods.clone().unwrap_or_default();
        }
        methods.sort_unstable();
        methods.dedup();
        // Sort by context then app id so the digest does not depend on the
        // order the API happened to return.
        required.sort_by(|a, b| {
            a.context
                .cmp(&b.context)
                .then_with(|| a.integration_id.cmp(&b.integration_id))
        });

        let mut rules = Self {
            required,
            strict_up_to_date,
            allowed_methods: methods,
            linear_history,
            merge_queue,
            thread_resolution,
            repository_methods,
            digest: String::new(),
        };
        rules.digest = canonical_digest(&rules);
        rules
    }

    /// Whether the rules require nothing at all.
    ///
    /// An empty required set is not proof of readiness: it is reported as a
    /// warning, and the check verdict still fails closed on an empty check set.
    #[must_use]
    pub fn has_no_required_checks(&self) -> bool {
        self.required.is_empty()
    }

    /// The merge methods a ruleset permits *and* the repository enables.
    #[must_use]
    pub fn usable_methods(&self) -> Vec<MergeMethod> {
        self.allowed_methods
            .iter()
            .copied()
            .filter(|method| {
                self.repository_methods
                    .as_ref()
                    .is_none_or(|enabled| enabled.contains(method))
            })
            .collect()
    }

    /// Whether the ruleset permits only methods the repository has disabled.
    ///
    /// A contradiction, never a silent "nothing to check": the server block is
    /// unambiguous, so it is reported as a blocker rather than an unknown.
    #[must_use]
    pub fn merge_method_unavailable(&self) -> bool {
        // Only a ruleset that names methods AND a repository set that was
        // actually reported can contradict each other.
        self.repository_methods.is_some()
            && !self.allowed_methods.is_empty()
            && self.usable_methods().is_empty()
    }

    /// Every way the head's checks fail the ruleset, or an empty vector.
    ///
    /// Three independent failures, all reported rather than short-circuited so
    /// one run tells the whole story:
    ///
    /// * **presence** — the required context has no check run at all;
    /// * **verdict** — a run publishes it but is not passing;
    /// * **identity** — a run publishes it and passes, but from an app the
    ///   ruleset did not name. A name-only gate lets any app publish
    ///   `CI / Required` and satisfy it, so this comparison fails closed when
    ///   the run carries no app id.
    ///
    /// A required context the ruleset names no app for is matched on the
    /// context alone, because that is all the server asked for.
    #[must_use]
    pub fn gate(&self, entries: &[super::checks::CheckEntry]) -> Vec<RuleViolation> {
        let mut violations = Vec::new();
        for required in &self.required {
            let matching: Vec<&super::checks::CheckEntry> = entries
                .iter()
                .filter(|entry| entry.name == required.context)
                .collect();
            if matching.is_empty() {
                violations.push(RuleViolation {
                    code: codes::REQUIRED_CHECK_MISSING,
                    context: required.context.clone(),
                    detail: "no check run publishes this required context".to_owned(),
                });
                continue;
            }
            let passing: Vec<&&super::checks::CheckEntry> = matching
                .iter()
                .filter(|entry| entry.verdict == super::checks::Verdict::Pass)
                .collect();
            if passing.is_empty() {
                violations.push(RuleViolation {
                    code: codes::REQUIRED_CHECK_FAILING,
                    context: required.context.clone(),
                    detail: "the required context is not passing".to_owned(),
                });
                continue;
            }
            let Some(expected) = required.integration_id else {
                continue;
            };
            if passing.iter().any(|entry| entry.app_id == Some(expected)) {
                continue;
            }
            let observed = passing.iter().find_map(|entry| entry.app_id);
            violations.push(RuleViolation {
                code: codes::REQUIRED_CHECK_APP_MISMATCH,
                context: required.context.clone(),
                detail: match observed {
                    Some(id) => format!(
                        "a passing run publishes this context from app {id}, not {expected}"
                    ),
                    None => "the passing run publishes this context with no app id".to_owned(),
                },
            });
        }
        violations
    }
}

/// Canonical digest over the normalised rules.
///
/// Takes the assembled struct so the digest can never disagree with the rules
/// it describes: adding a rule field without pointing it here is not possible.
fn canonical_digest(rules: &MergeRules) -> String {
    let mut hasher = Sha256::new();
    for context in &rules.required {
        let line = format!(
            "required={}|{}",
            context.context,
            context
                .integration_id
                .map_or_else(|| "-".to_owned(), |id| id.to_string())
        );
        hasher.update(line.as_bytes());
        hasher.update(b"\n");
    }
    hasher.update(format!("strict={}\n", rules.strict_up_to_date).as_bytes());
    let methods: Vec<&str> = rules.allowed_methods.iter().map(|m| m.as_str()).collect();
    hasher.update(format!("methods={}\n", methods.join(",")).as_bytes());
    hasher.update(format!("linear={}\n", rules.linear_history).as_bytes());
    hasher.update(format!("queue={}\n", rules.merge_queue).as_bytes());
    hasher.update(format!("threads={}\n", rules.thread_resolution).as_bytes());
    format!("sha256:{}", hex::encode(hasher.finalize()))
}
