//! Individual sweep steps (#471).
//!
//! Every step composes existing functions and commands: there is no second
//! `GitHub` client, no second check classifier, and no second policy
//! interpreter. Mutation commands are issued exactly once, deterministically;
//! a failed mutation blocks, never retries.

use std::path::Path;
use std::time::{Duration, Instant};

use crate::pr::checks::{self, ChecksReport};
use crate::pr::gh::{self, PermissionOutcome, PrListEntry};
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
/// The server's own `is_bot` flag is authoritative and costs zero permission
/// calls (which cannot even answer for an `app/<slug>` login); documented bot
/// names are the fallback for payloads that omit the flag. For a non-bot login
/// the collaborator-permission answer decides: an authorized role is owned, a
/// `404` is a real third party, and an unreadable answer (`403`) is an unknown
/// identity — never an implicit pass.
pub fn classify_author(root: &Path, entry: &PrListEntry) -> Owner {
    let Some(author) = &entry.author else {
        return Owner::UnknownIdentity;
    };
    classify_author_with(author, |login| gh::user_permission_outcome(root, login))
}

/// The classification fold, with the permission read injected.
///
/// Bot classification happens before `permission` is ever consulted, so an
/// App-authored PR (whose `app/<slug>` login the collaborator endpoint cannot
/// even answer for) costs zero API calls. The closure is the only I/O, which
/// lets the acceptance table run without a fake `gh`.
fn classify_author_with<F>(author: &crate::pr::gh::CommentAuthor, permission: F) -> Owner
where
    F: FnOnce(&str) -> anyhow::Result<PermissionOutcome>,
{
    let login = author.login.trim();
    if login.is_empty() {
        return Owner::UnknownIdentity;
    }
    if crate::pr::readiness::is_bot_author(login, author.is_bot) {
        return Owner::Owned;
    }
    match permission(login) {
        Ok(PermissionOutcome::Role(role)) if is_authorized_role(&role) => Owner::Owned,
        // An empty role is the endpoint's answer with no role attached: it
        // proves no collaboration relationship, the same as a `404`.
        Ok(PermissionOutcome::Role(_) | PermissionOutcome::NotCollaborator) => Owner::External,
        Ok(PermissionOutcome::Unknown(_)) | Err(_) => Owner::UnknownIdentity,
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

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use std::cell::Cell;

    use super::*;
    use crate::pr::gh::CommentAuthor;

    fn author(login: &str, is_bot: bool) -> CommentAuthor {
        CommentAuthor {
            login: login.to_owned(),
            is_bot,
        }
    }

    /// Classifies with a recording permission read, asserting call counts.
    fn classify(author: &CommentAuthor, outcome: PermissionOutcome) -> (Owner, usize) {
        let calls = Cell::new(0);
        let owner = classify_author_with(author, |_login| {
            calls.set(calls.get() + 1);
            Ok(outcome)
        });
        (owner, calls.get())
    }

    #[test]
    fn an_app_authored_dependency_bot_is_owned_with_zero_permission_calls() {
        // The regression from #514: `gh` reports the login as `app/dependabot`
        // and the collaborator endpoint answers neither spelling, so any
        // permission read is both useless and fatal.
        let (owner, calls) = classify(
            &author("app/dependabot", true),
            PermissionOutcome::Unknown("HTTP 403: rate limit exceeded".to_owned()),
        );
        assert_eq!(owner, Owner::Owned);
        assert_eq!(calls, 0, "bot classification must not read permission");
    }

    #[test]
    fn the_server_flag_owns_a_bot_whose_name_matches_no_documented_form() {
        let (owner, calls) = classify(
            &author("some-app", true),
            PermissionOutcome::NotCollaborator,
        );
        assert_eq!(owner, Owner::Owned);
        assert_eq!(calls, 0);
    }

    #[test]
    fn a_non_bot_author_is_classified_by_the_permission_answer() {
        for (role, expected) in [
            ("admin", Owner::Owned),
            ("write", Owner::Owned),
            ("maintain", Owner::Owned),
            ("owner", Owner::Owned),
            ("read", Owner::External),
            ("", Owner::External),
        ] {
            let (owner, calls) = classify(
                &author("octocat", false),
                PermissionOutcome::Role(role.to_owned()),
            );
            assert_eq!(owner, expected, "role {role:?}");
            assert_eq!(calls, 1);
        }
    }

    #[test]
    fn a_not_found_answer_is_a_real_third_party_and_a_forbidden_one_is_unknown() {
        // 404 proves no collaboration relationship; 403 proves nothing at all.
        let (external, _) = classify(
            &author("octocat", false),
            PermissionOutcome::NotCollaborator,
        );
        assert_eq!(external, Owner::External);
        let (unknown, _) = classify(
            &author("octocat", false),
            PermissionOutcome::Unknown("HTTP 403".to_owned()),
        );
        assert_eq!(
            unknown,
            Owner::UnknownIdentity,
            "an unreadable author is never owned"
        );
    }

    #[test]
    fn a_missing_or_blank_author_is_an_unknown_identity() {
        let entry = PrListEntry {
            number: 1,
            title: String::new(),
            url: String::new(),
            author: None,
            created_at: String::new(),
            updated_at: String::new(),
            is_draft: false,
            state: "OPEN".to_owned(),
            base_ref_name: String::new(),
            base_ref_oid: String::new(),
            head_ref_name: String::new(),
            head_ref_oid: String::new(),
            merge_state_status: String::new(),
            labels: Vec::new(),
            head_repository: None,
        };
        assert_eq!(
            classify_author(Path::new("."), &entry),
            Owner::UnknownIdentity
        );
        let blank = PrListEntry {
            author: Some(author("   ", false)),
            ..entry
        };
        assert_eq!(
            classify_author(Path::new("."), &blank),
            Owner::UnknownIdentity,
            "a blank login is unreadable, not a third party"
        );
    }
}
