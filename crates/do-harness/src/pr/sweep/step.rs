//! Individual sweep steps (#471).
//!
//! Every step composes existing functions and commands: there is no second
//! `GitHub` client, no second check classifier, and no second policy
//! interpreter. Mutation commands are issued exactly once, deterministically;
//! a failed mutation blocks, never retries.

use std::path::Path;
use std::time::{Duration, Instant};

use crate::pr::checks::{self, ChecksReport};
use crate::pr::gh::{self, PrListEntry};
use crate::pr::readiness::AuthorizationReason;
use crate::pr::waiver_comment::is_authorized_role;

/// Bound for post-merge main verification, regardless of `--wait`.
pub const MAX_POST_MERGE_WAIT_SECONDS: u64 = 900;

/// Owner classification for a PR author.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Owner {
    /// The sweep may update/close/merge this PR.
    Owned,
    /// A third-party author: report, never touch.
    External,
    /// The author or their permission could not be determined.
    UnknownIdentity,
}

/// Classifies whether a PR's author is sweep-owned.
///
/// Bot accounts (`[bot]`, dependabot, renovate) and authors with
/// admin/write/maintain/owner permission are owned. Permission lookup failure
/// is an unknown identity, never an implicit pass.
pub fn classify_author(root: &Path, entry: &PrListEntry) -> Owner {
    let Some(author) = &entry.author else {
        return Owner::UnknownIdentity;
    };
    if author.login.trim().is_empty() {
        return Owner::UnknownIdentity;
    }
    if crate::pr::readiness::is_bot_login(&author.login) {
        return Owner::Owned;
    }
    match gh::user_permission(root, &author.login) {
        Ok(role) if is_authorized_role(&role) => Owner::Owned,
        Ok(_) => Owner::External,
        Err(_) => Owner::UnknownIdentity,
    }
}

/// Whether `labels` carries a hard-stop label, and which one.
#[must_use]
pub fn held_label(labels: &[crate::pr::gh::PrLabel]) -> Option<String> {
    labels
        .iter()
        .map(|label| label.name.as_str())
        .find(|name| {
            crate::pr::readiness::HELD_LABELS
                .iter()
                .any(|held| name.eq_ignore_ascii_case(held))
        })
        .map(str::to_owned)
}

/// Outcome of the bounded CI wait.
pub enum WaitOutcome {
    /// Every check is a pass.
    Passed,
    /// A required check failed.
    Failed(ChecksReport),
    /// The check set cannot be proven.
    Unknown,
    /// The verdict was still pending when the deadline passed.
    Deadline,
}

/// Polls the typed check classifier until it settles or the bound expires.
///
/// The verdict is bound to `head_sha`; `conflicts` marks the incomplete-set
/// case (`GitHub` schedules no workflow checks on a conflicting head). A
/// transient API failure keeps polling until the deadline — unavailable
/// evidence is never a pass.
pub fn wait_for_checks(
    root: &Path,
    pr: u64,
    head_sha: &str,
    conflicts: bool,
    wait_seconds: u64,
) -> WaitOutcome {
    let deadline = Instant::now() + Duration::from_secs(wait_seconds);
    loop {
        if let Ok(report) = checks::evaluate_for_head(root, pr, head_sha, conflicts) {
            match report.verdict.as_str() {
                "PASS" => return WaitOutcome::Passed,
                "FAIL" => return WaitOutcome::Failed(report),
                "PENDING" => {}
                _ => return WaitOutcome::Unknown,
            }
        }
        let now = Instant::now();
        if now >= deadline {
            return WaitOutcome::Deadline;
        }
        std::thread::sleep(poll_step(deadline.saturating_duration_since(now)));
    }
}

/// Post-merge main-branch verification verdict.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MainVerdict {
    /// The merged commit's runs are all successful.
    Pass,
    /// A run on the merged commit failed: stop the sweep.
    Regression,
    /// Runs are empty, pending, or unreadable at the deadline.
    Unknown,
}

/// Waits for the exact-merge main verification (bounded at 900 seconds).
///
/// An empty run list is not a pass; a failure is a regression. Unknown or
/// pending state at the deadline is an unknown halt.
pub fn verify_main_after_merge(root: &Path, merge_sha: &str, wait_seconds: u64) -> MainVerdict {
    let bound = wait_seconds.min(MAX_POST_MERGE_WAIT_SECONDS);
    let deadline = Instant::now() + Duration::from_secs(bound);
    loop {
        if let Ok(page) = gh::check_runs_page(root, merge_sha) {
            let failed = page.check_runs.iter().any(|run| {
                matches!(
                    run.conclusion.as_deref(),
                    Some("failure" | "timed_out" | "action_required" | "startup_failure")
                )
            });
            if failed {
                return MainVerdict::Regression;
            }
            let incomplete = page
                .check_runs
                .iter()
                .any(|run| run.status != "completed" || run.conclusion.is_none());
            if !page.check_runs.is_empty() && !incomplete {
                return MainVerdict::Pass;
            }
        }
        let now = Instant::now();
        if now >= deadline {
            return MainVerdict::Unknown;
        }
        std::thread::sleep(poll_step(deadline.saturating_duration_since(now)));
    }
}

/// Sleep step for a bounded wait: at most 15 s, at least 1 s, shrinking as
/// the deadline approaches so short waits finish quickly.
fn poll_step(remaining: Duration) -> Duration {
    let seconds = remaining.as_secs().max(1);
    Duration::from_secs((seconds / 4).clamp(1, 15))
}

/// Derives the explicit next action for a blocked PR.
#[must_use]
pub fn next_action(pr: u64, reasons: &[AuthorizationReason]) -> Option<String> {
    for reason in reasons {
        match reason.code.as_str() {
            crate::pr::readiness::codes::FINDINGS_UNTRACKED => {
                return Some(format!("do-harness pr findings {pr} --issues create"));
            }
            crate::pr::readiness::codes::CHECKS_FAILED => {
                return Some("fix or rerun the failing required check; never clear it".to_owned());
            }
            crate::pr::readiness::codes::POLICY_DRIFT => {
                return Some(
                    "apply the authorized server ruleset change, then do-harness doctor pr-policy"
                        .to_owned(),
                );
            }
            crate::pr::readiness::codes::CONVERSATION_UNRESOLVED
            | crate::pr::readiness::codes::CONVERSATION_NEEDS_TRIAGE
            | crate::pr::readiness::codes::REVIEW_CHANGES_REQUESTED => {
                return Some(
                    "reply and resolve conversations (or record a disposition via do-harness pr disposition)"
                        .to_owned(),
                );
            }
            crate::pr::readiness::codes::AUTO_MERGE_ARMED => {
                return Some("rerun with --execute to disarm the auto-merge request".to_owned());
            }
            crate::pr::readiness::codes::MERGE_STATE_DIRTY => {
                return Some(
                    "update the branch, or resolve the merge conflict in an authorized work session"
                        .to_owned(),
                );
            }
            _ => {}
        }
    }
    None
}

/// Whether a merge-impact report supports closure and carries the full SHA pair.
#[must_use]
pub fn closure_eligible(impact: &crate::pr::no_effect::MergeImpactReport) -> bool {
    impact.verdict.is_no_tree_effect()
        && impact.base_verified
        && impact.base_sha.is_some()
        && impact.head_sha.is_some()
}

/// The no-tree-effect closure comment: method and the exact compared pair.
#[must_use]
pub fn no_effect_comment(impact: &crate::pr::no_effect::MergeImpactReport) -> String {
    let base = impact.base_sha.as_deref().unwrap_or("");
    let head = impact.head_sha.as_deref().unwrap_or("");
    format!(
        "Closed automatically: no tree effect (merge-impact NO_TREE_EFFECT with verified base; base {base} head {head})."
    )
}

/// The duplicate-supersede closure comment.
#[must_use]
pub fn supersede_comment(keeper: u64, files: &[String], rule: &str) -> String {
    let list = files.join(", ");
    format!(
        "Superseded by #{keeper}: same {list} change on the same base blob, kept by rule ({rule}) — no tree effect remains for this member."
    )
}
