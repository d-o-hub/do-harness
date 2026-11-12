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

use serde::{Deserialize, Serialize};

use super::gh::{CheckRun, CommitStatus};

#[path = "checks_eval.rs"]
mod eval;
#[allow(unused_imports)]
pub use eval::evaluate_for_head_with_base;
pub use eval::{evaluate_for_head, run};

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

// Re-exported so `pr::checks::*` keeps meaning the same thing to every caller
// (the tests read the file names, the evaluator reads `load_policy`).
#[allow(unused_imports)]
pub use super::checks_policy::{ALLOW_FILE, ChecksPolicy, POLICY_DIR, POLICY_FILE, load_policy};

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
        let matching: Vec<&CheckEntry> = entries
            .iter()
            .filter(|entry| entry.name == *required)
            .collect();
        if matching.iter().any(|entry| entry.verdict == Verdict::Pass) {
            continue;
        }
        // Distinguish "the required context said skipped" from "no run
        // publishes it": both block, but only one of them is the operator's
        // fixture to fix, and a bare `missing_required_check` for a context the
        // API did report reads as a bug in the tool.
        let skipped = matching
            .iter()
            .any(|entry| entry.reason.as_deref() == Some("skipped_required"));
        if skipped {
            blockers.push(CheckBlocker {
                code: "required_check_skipped".to_owned(),
                message: format!(
                    "required check '{required}' was skipped; a required context can never be satisfied by a skip"
                ),
            });
        } else {
            blockers.push(CheckBlocker {
                code: "missing_required_check".to_owned(),
                message: format!("required check '{required}' is missing or not passing"),
            });
        }
        verdict = verdict.merge(Verdict::Unknown);
    }

    // `allow_skips` naming a required context is a contradiction, not a knob:
    // the required entry always wins, so the allowlist entry can never take
    // effect. Say so instead of leaving a reviewer to believe the gate moved.
    for name in policy.contradictory_skips() {
        blockers.push(CheckBlocker {
            code: "contradictory_required_skip".to_owned(),
            message: format!(
                "'{name}' is in allow_skips but is a required check; the required set always wins, so remove the allowlist entry"
            ),
        });
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

/// Classifies a `skipped`/`neutral` conclusion against the required set.
///
/// A context the server requires is blocking whatever its conclusion, so a
/// required skip stays a failure and is labelled as such. Everything else is
/// optional CI: an unrelated maintenance job that concludes `skipped` says
/// nothing about whether the change may merge, and treating it as a failure is
/// a false blocker that no repository without a hand-written allowlist could
/// clear. The skip is still reported — `Skip`, with the app slug and conclusion
/// carried on the entry — so it never disappears from the report.
fn classify_skip(name: &str, policy: &ChecksPolicy) -> (Verdict, Option<String>) {
    if policy.is_required(name) {
        (Verdict::Fail, Some("skipped_required".to_owned()))
    } else if !policy.required_known {
        // The required set is unknown, so "nothing requires this context" is
        // not a fact. Reporting a skip as a pass here would widen what merges
        // exactly when the tool failed to read the rules — the opposite of the
        // fail-closed contract.
        (
            Verdict::Unknown,
            Some("skipped, but the required set could not be read".to_owned()),
        )
    } else if policy.allow_skips.contains(name) {
        (Verdict::Skip, Some("allowlisted skip".to_owned()))
    } else {
        (Verdict::Skip, Some("optional skip".to_owned()))
    }
}

fn classify_run(run: &CheckRun, policy: &ChecksPolicy) -> (Verdict, Option<String>) {
    if run.status != "completed" {
        return (Verdict::Pending, Some(format!("status={}", run.status)));
    }
    match run.conclusion.as_deref() {
        Some("success") => (Verdict::Pass, None),
        Some("skipped" | "neutral") => classify_skip(&run.name, policy),
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
        "neutral" => classify_skip(&status.context, policy),
        other => (Verdict::Unknown, Some(format!("unknown state {other}"))),
    }
}

#[cfg(test)]
#[path = "checks_tests.rs"]
mod tests;
