//! Deterministic CI truth for merge authorization (#465).
//!
//! One typed classifier decides whether a pull request's checks authorize a
//! merge. It binds the verdict to the current head SHA, reads every page of
//! check runs and legacy commit statuses, verifies the API's `total_count`,
//! and fails closed (`UNKNOWN`) on anything it cannot prove: an unreadable or
//! truncated response, an empty check set without an explicit policy, a
//! missing required check, or a head that moved during inspection.
//!
//! The shell skill calls this module through `do-harness pr checks`; it does
//! not keep a second parser.

use std::collections::BTreeSet;
use std::path::Path;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use super::gh::{CheckRun, CommitStatus};

#[path = "checks_eval.rs"]
mod eval;
pub use eval::{evaluate_for_head, run};

/// Directory holding the auditable CI policy, relative to the repository root.
pub const POLICY_DIR: &str = ".git/pr-triage";
/// TOML policy file: `allow_no_ci`, `required`, and `allow_skips`.
pub const POLICY_FILE: &str = "checks-policy.toml";
/// Legacy one-name-per-line skip allowlist, still honoured.
pub const ALLOW_FILE: &str = "checks-allow.txt";

/// How many times a moving head is re-read before the snapshot is declared
/// unstable.
const MAX_HEAD_ATTEMPTS: usize = 3;

/// The merge-gating verdict for one check, status, or the whole suite.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum Verdict {
    /// The check succeeded, or an allowlisted skip.
    Pass,
    /// The check failed, was cancelled, or is an unallowlisted skip.
    Fail,
    /// The check has not completed.
    Pending,
    /// The state cannot be proven; never merge on it.
    Unknown,
    /// A skipped/neutral check explicitly allowed by policy.
    Skip,
}

impl Verdict {
    /// Stable uppercase label used in text and JSON output.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pass => "PASS",
            Self::Fail => "FAIL",
            Self::Pending => "PENDING",
            Self::Unknown => "UNKNOWN",
            Self::Skip => "SKIP",
        }
    }

    /// Severity used to fold many checks into one verdict.
    fn rank(self) -> u8 {
        match self {
            Self::Pass | Self::Skip => 0,
            Self::Unknown => 1,
            Self::Pending => 2,
            Self::Fail => 3,
        }
    }

    /// Returns the more severe of two verdicts.
    fn merge(self, other: Self) -> Self {
        if other.rank() > self.rank() {
            other
        } else {
            self
        }
    }
}

/// One classified check run or commit status.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckEntry {
    /// Check name or status context.
    pub name: String,
    /// App slug for a check run, or `status` for a legacy status.
    pub source: String,
    /// Numeric app id that published the run, when `GitHub` reports one.
    ///
    /// A ruleset requires `(context, integration_id)`; matching on the name
    /// alone would let any app satisfy a required context, so the identity is
    /// carried here for the comparison.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app_id: Option<u64>,
    /// `check` or `status`.
    pub kind: String,
    /// Raw status/state reported by `GitHub`.
    pub state: String,
    /// Raw conclusion for a check run.
    pub conclusion: Option<String>,
    /// Merge-gating verdict.
    pub verdict: Verdict,
    /// Machine-readable reason for a non-pass verdict or an allowed skip.
    pub reason: Option<String>,
    /// Exact rerun command for a cancelled check.
    pub rerun_command: Option<String>,
    /// Check-run or status target URL.
    pub url: Option<String>,
}

/// A machine-readable reason the suite is not a pass.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckBlocker {
    /// Stable code, e.g. `no_checks_reported` or `missing_required_check`.
    pub code: String,
    /// Human-readable explanation.
    pub message: String,
}

/// The auditable policy applied to a repository's checks.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PolicySummary {
    /// An empty check set is a pass (explicit opt-in).
    pub allow_no_ci: bool,
    /// Check names that must be present and passing.
    pub required: Vec<String>,
    /// Check names whose skipped/neutral conclusion is acceptable.
    pub allow_skips: Vec<String>,
}

/// The complete CI verdict for one PR head.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChecksReport {
    /// Pull request number.
    pub pr: u64,
    /// Head SHA the verdict is bound to.
    pub head_sha: String,
    /// Folded verdict.
    pub verdict: Verdict,
    /// Every classified check and status.
    pub checks: Vec<CheckEntry>,
    /// Explicit blockers; empty only for a pass.
    pub blockers: Vec<CheckBlocker>,
    /// Policy that produced the verdict.
    pub policy: PolicySummary,
}

/// Loaded policy, with the skip allowlist as a set.
#[derive(Debug, Clone, Default)]
pub struct ChecksPolicy {
    /// An empty check set is a pass.
    pub allow_no_ci: bool,
    /// Check names that must be present and passing.
    pub required: Vec<String>,
    /// Check names whose skipped/neutral conclusion is acceptable.
    pub allow_skips: BTreeSet<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PolicyFile {
    #[serde(default)]
    allow_no_ci: bool,
    #[serde(default)]
    required: Vec<String>,
    #[serde(default)]
    allow_skips: Vec<String>,
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
        policy.required = file.required;
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

/// Classifies check runs and commit statuses into a merge-gating report.
///
/// Pure over its inputs so the decision that authorizes a merge is covered on
/// every platform, not only where a fake `gh` can be put on `PATH`.
#[must_use]
pub fn classify(
    pr: u64,
    head_sha: &str,
    check_runs: Vec<CheckRun>,
    statuses: Vec<CommitStatus>,
    policy: &ChecksPolicy,
    conflicts: bool,
) -> ChecksReport {
    let mut entries = classify_runs(check_runs, policy);
    entries.extend(classify_statuses(statuses, policy));
    let (verdict, blockers) = apply_policy(&entries, policy, head_sha, conflicts);
    ChecksReport {
        pr,
        head_sha: head_sha.to_owned(),
        verdict,
        checks: entries,
        blockers,
        policy: PolicySummary {
            allow_no_ci: policy.allow_no_ci,
            required: policy.required.clone(),
            allow_skips: policy.allow_skips.iter().cloned().collect(),
        },
    }
}

fn classify_runs(check_runs: Vec<CheckRun>, policy: &ChecksPolicy) -> Vec<CheckEntry> {
    let mut entries = Vec::new();
    let mut seen_ids = BTreeSet::new();
    for run in check_runs {
        if let Some(id) = run.id {
            if !seen_ids.insert(id) {
                continue;
            }
        }
        let (verdict, reason) = classify_run(&run, policy);
        let source = run
            .app
            .as_ref()
            .and_then(|app| app.slug.clone())
            .unwrap_or_else(|| "unknown".to_owned());
        let app_id = run.app.as_ref().and_then(|app| app.id);
        entries.push(CheckEntry {
            name: run.name.clone(),
            source,
            app_id,
            kind: "check".to_owned(),
            state: run.status.clone(),
            conclusion: run.conclusion.clone(),
            verdict,
            reason,
            rerun_command: (verdict == Verdict::Fail
                && run.conclusion.as_deref() == Some("cancelled"))
            .then(|| run.rerun_command()),
            url: run.html_url.clone(),
        });
    }
    entries
}

fn classify_statuses(statuses: Vec<CommitStatus>, policy: &ChecksPolicy) -> Vec<CheckEntry> {
    let mut entries = Vec::new();
    // The status API returns newest first; the first occurrence of a context is
    // the current one, so later duplicates are stale and must not override it.
    let mut seen_contexts = BTreeSet::new();
    for status in statuses {
        if !seen_contexts.insert(status.context.clone()) {
            continue;
        }
        let (verdict, reason) = classify_status(&status, policy);
        entries.push(CheckEntry {
            name: status.context.clone(),
            source: "status".to_owned(),
            // The commit-status API carries no app identity, so a required
            // context that names an `integration_id` cannot be satisfied here
            // and the comparison fails closed.
            app_id: None,
            kind: "status".to_owned(),
            state: status.state.clone(),
            conclusion: None,
            verdict,
            reason,
            rerun_command: None,
            url: status.target_url.clone(),
        });
    }
    entries
}

fn apply_policy(
    entries: &[CheckEntry],
    policy: &ChecksPolicy,
    head_sha: &str,
    conflicts: bool,
) -> (Verdict, Vec<CheckBlocker>) {
    let mut verdict = Verdict::Pass;
    for entry in entries {
        verdict = verdict.merge(entry.verdict);
    }

    let mut blockers = Vec::new();
    if entries.is_empty() {
        if policy.allow_no_ci {
            // Explicit, auditable opt-out: an empty suite is a pass.
        } else {
            verdict = verdict.merge(Verdict::Unknown);
            blockers.push(CheckBlocker {
                code: "no_checks_reported".to_owned(),
                message: format!(
                    "no check runs or commit statuses were reported for {head_sha}; CI is expected (set allow_no_ci in {POLICY_DIR}/{POLICY_FILE} to opt out)"
                ),
            });
        }
    }

    for required in &policy.required {
        let present = entries
            .iter()
            .any(|entry| entry.name == *required && entry.verdict == Verdict::Pass);
        if !present {
            verdict = verdict.merge(Verdict::Unknown);
            blockers.push(CheckBlocker {
                code: "missing_required_check".to_owned(),
                message: format!("required check '{required}' is missing or not passing"),
            });
        }
    }

    if conflicts && !entries.is_empty() {
        verdict = verdict.merge(Verdict::Unknown);
        blockers.push(CheckBlocker {
            code: "checks_incomplete".to_owned(),
            message: format!(
                "checks are incomplete: merge conflicts stop workflow checks from being scheduled ({} check run(s) reported)",
                entries.len()
            ),
        });
    }

    for entry in entries {
        let reason = entry.reason.as_deref().unwrap_or("unknown");
        match entry.verdict {
            Verdict::Fail => blockers.push(CheckBlocker {
                code: "check_failed".to_owned(),
                message: format!("{} '{}' failed ({reason})", entry.kind, entry.name),
            }),
            Verdict::Pending => blockers.push(CheckBlocker {
                code: "check_pending".to_owned(),
                message: format!("{} '{}' is pending ({reason})", entry.kind, entry.name),
            }),
            Verdict::Unknown => blockers.push(CheckBlocker {
                code: "check_unknown".to_owned(),
                message: format!("{} '{}' is unknown ({reason})", entry.kind, entry.name),
            }),
            Verdict::Pass | Verdict::Skip => {}
        }
    }
    (verdict, blockers)
}

fn classify_run(run: &CheckRun, policy: &ChecksPolicy) -> (Verdict, Option<String>) {
    if run.status != "completed" {
        return (Verdict::Pending, Some(format!("status={}", run.status)));
    }
    match run.conclusion.as_deref() {
        Some("success") => (Verdict::Pass, None),
        Some("skipped" | "neutral") => {
            if policy.allow_skips.contains(&run.name) {
                (Verdict::Skip, Some("allowlisted skip".to_owned()))
            } else {
                (Verdict::Fail, Some("unallowlisted skip".to_owned()))
            }
        }
        Some("cancelled") => (Verdict::Fail, Some("cancelled".to_owned())),
        Some("timed_out") => (Verdict::Fail, Some("timed_out".to_owned())),
        Some("action_required") => (Verdict::Fail, Some("action_required".to_owned())),
        Some("startup_failure") => (Verdict::Fail, Some("startup_failure".to_owned())),
        Some("failure") => (Verdict::Fail, Some("failure".to_owned())),
        Some(other) => (
            Verdict::Unknown,
            Some(format!("unknown conclusion {other}")),
        ),
        None => (
            Verdict::Unknown,
            Some("completed without conclusion".to_owned()),
        ),
    }
}

fn classify_status(status: &CommitStatus, policy: &ChecksPolicy) -> (Verdict, Option<String>) {
    match status.state.as_str() {
        "success" => (Verdict::Pass, None),
        "pending" => (Verdict::Pending, Some("pending".to_owned())),
        "failure" | "error" => (Verdict::Fail, Some(status.state.clone())),
        "neutral" => {
            if policy.allow_skips.contains(&status.context) {
                (Verdict::Skip, Some("allowlisted skip".to_owned()))
            } else {
                (Verdict::Fail, Some("unallowlisted skip".to_owned()))
            }
        }
        other => (Verdict::Unknown, Some(format!("unknown state {other}"))),
    }
}

#[cfg(test)]
#[path = "checks_tests.rs"]
mod tests;
