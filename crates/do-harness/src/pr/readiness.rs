//! Deterministic PR merge readiness inspection (#240, #466).
//!
//! Evaluates:
//! - Merge state (`mergeStateStatus`, `mergeable`), PR state, draft and labels
//! - CI check runs and commit statuses through the typed classifier
//! - Reviews, review threads, issue comments and durable dispositions (#469)
//! - Findings provenance against the verified base baseline (#470)
//! - Server merge policy drift (#467)
//! - Read-only ancestry of the captured `(base, head)` commit pair
//!
//! The merge authority is [`model::Authorization`]; `ready` is presentation.

use std::path::Path;

use anyhow::{Context, Result};

use super::ancestry;
use super::gh;
use super::policy::{self, PolicyDecision};
#[allow(unused_imports)]
pub use super::waiver_comment::{CodecovStatus, CodecovSummary, WaiverAuditRecord};
use crate::CliError;
use crate::report::Format;

/// Logins that post Codecov's own report comments: the app (`codecov[bot]`),
/// the legacy bot user (`codecov`), and the comment-bot account.
const CODECOV_AUTHORS: [&str; 3] = ["codecov", "codecov[bot]", "codecov-commenter"];

/// Attempts allowed before a snapshot that keeps moving is declared unstable.
const MAX_SNAPSHOT_ATTEMPTS: usize = 3;

pub(crate) fn is_codecov_author(login: &str) -> bool {
    CODECOV_AUTHORS.contains(&login)
}

/// Whether `login` is a bot account: `GitHub` Apps (`[bot]` suffix) and the
/// well-known dependency bots.
pub(crate) fn is_bot_login(login: &str) -> bool {
    login.ends_with("[bot]") || login == "dependabot" || login == "renovate"
}

#[cfg(test)]
mod tests;

mod authorization;
mod collect;
mod model;
mod reason;
mod receipt;

#[allow(unused_imports)]
pub use authorization::{
    Authorization, AuthorizationDecision, AuthorizationReason, MergeReceipt, ReceiptVerdict, codes,
    extract_receipt, validate_receipt,
};
#[allow(unused_imports)]
pub use model::{
    ActionableComment, CancelledCheck, CheckSummary, ConversationSummary, FailedCheck,
    FindingsSummary, MergeState, ReadinessReport, UnresolvedThread,
};

use collect::{evaluate_checks, evaluate_conversations, evaluate_findings_block};

/// Evaluates merge readiness for PR `number`.
///
/// # Errors
///
/// Returns an error when querying `GitHub` fails.
#[allow(dead_code)]
pub fn evaluate(root: &Path, number: u64) -> Result<ReadinessReport> {
    evaluate_with_receipt(root, number, None)
}

/// Evaluates readiness, optionally requiring a previously issued receipt.
///
/// With `expected`, the report's authorization must match the supplied
/// receipt: the snapshot is recomputed from scratch and compared, so the file
/// can only prove that nothing moved, never that something was allowed.
///
/// # Errors
///
/// Returns an error when querying `GitHub` fails or the receipt file cannot be
/// read.
#[allow(clippy::too_many_lines)]
pub fn evaluate_with_receipt(
    root: &Path,
    number: u64,
    expected: Option<&Path>,
) -> Result<ReadinessReport> {
    let supplied = match expected {
        Some(path) => Some(
            std::fs::read_to_string(path)
                .with_context(|| format!("failed to read receipt file {}", path.display()))?,
        ),
        None => None,
    };

    let mut last_moved: Option<String> = None;
    for attempt in 0..MAX_SNAPSHOT_ATTEMPTS {
        let view = gh::readiness_view(root, number)?;
        let mut blockers = Vec::new();

        // 1. Merge state
        if view.merge_state_status != "CLEAN" {
            blockers.push(format!(
                "merge state is '{}' (expected 'CLEAN')",
                view.merge_state_status
            ));
        }
        if view.mergeable != "MERGEABLE" {
            blockers.push(format!(
                "mergeability is '{}' (expected 'MERGEABLE')",
                view.mergeable
            ));
        }
        let conflicts = view.merge_state_status == "DIRTY" || view.mergeable == "CONFLICTING";
        let merge_state = MergeState {
            status: view.merge_state_status.clone(),
            mergeable: view.mergeable.clone(),
            auto_merge: view.auto_merge_request.is_some(),
        };

        // 2. Review decision
        if let Some(decision) = &view.review_decision {
            if decision == "CHANGES_REQUESTED" {
                blockers.push("review decision is 'CHANGES_REQUESTED'".to_owned());
            } else if decision == "REVIEW_REQUIRED" {
                blockers.push("review decision is 'REVIEW_REQUIRED'".to_owned());
            }
        }

        // 3. Checks, 4. conversations, 5. findings
        let checks = evaluate_checks(root, number, &view.head_ref_oid, conflicts, &mut blockers)?;
        let (conversations, codecov) = evaluate_conversations(root, number, &view, &mut blockers)?;
        let findings = evaluate_findings_block(root, &view, &mut blockers);

        // 6. Server policy and ancestry proof
        let policy_report = policy::evaluate(root).ok();
        let (policy_known, policy_ok, policy_revision) = match &policy_report {
            Some(report) => (
                report.decision != PolicyDecision::Unknown,
                report.decision == PolicyDecision::Pass,
                report.policy_sha256.clone(),
            ),
            None => (false, false, None),
        };
        let ancestry_proof = ancestry::compare(root, &view.base_ref_oid, &view.head_ref_oid).ok();

        // 7. Re-read the mutable refs: a snapshot that moved is not a snapshot.
        let recheck = gh::readiness_view(root, number)
            .context("failed to recheck PR state after metadata collection")?;
        let moved = recheck.head_ref_oid != view.head_ref_oid
            || recheck.base_ref_oid != view.base_ref_oid
            || recheck.state != view.state
            || recheck.is_draft != view.is_draft
            || recheck.merge_state_status != view.merge_state_status
            || recheck.mergeable != view.mergeable
            || recheck.labels.len() != view.labels.len();
        if moved {
            let message = format!(
                "PR state moved during inspection (head {} -> {}, base {} -> {})",
                view.head_ref_oid, recheck.head_ref_oid, view.base_ref_oid, recheck.base_ref_oid
            );
            if attempt + 1 < MAX_SNAPSHOT_ATTEMPTS {
                last_moved = Some(message);
                continue;
            }
            let authorization = authorization::unstable(message);
            return Ok(ReadinessReport {
                pr: number,
                head_ref_oid: view.head_ref_oid.clone(),
                collection_complete: false,
                ready: false,
                merge_state,
                checks,
                conversations,
                codecov,
                findings,
                authorization,
                blockers,
            });
        }

        // 8. Authorization
        let now_unix = authorization::now_unix();
        let inputs = authorization::SnapshotInputs {
            pr: number,
            view: &view,
            merge_state: &merge_state,
            checks: &checks,
            conversations: &conversations,
            findings: &findings,
            policy_revision,
            policy_ok,
            policy_known,
            ancestry: ancestry_proof.as_ref(),
            now_unix,
        };
        let authorization = authorization::authorize(&inputs);
        let authorization = match supplied.as_deref() {
            Some(document) => {
                authorization::apply_expected_receipt(authorization, document, now_unix)
            }
            None => authorization,
        };

        let ready = authorization.decision == AuthorizationDecision::MergeAllowed;
        return Ok(ReadinessReport {
            pr: number,
            head_ref_oid: view.head_ref_oid.clone(),
            collection_complete: conversations.complete,
            ready,
            merge_state,
            checks,
            conversations,
            codecov,
            findings,
            authorization,
            blockers,
        });
    }

    anyhow::bail!(
        "{}",
        last_moved.unwrap_or_else(|| {
            format!("failed to obtain a stable snapshot after {MAX_SNAPSHOT_ATTEMPTS} attempts")
        })
    )
}

/// Runs the ready command and prints the report in `format`.
///
/// Returns `Ok(())` when the authorization is `MERGE_ALLOWED`.
///
/// # Errors
///
/// Returns [`CliError::Usage`] (exit 2) when the snapshot or a supplied receipt
/// cannot be judged, and [`CliError::Verify`] (exit 1) when a known blocker
/// prevents merging.
pub fn run(
    root: &Path,
    number: u64,
    expected_receipt: Option<&Path>,
    format: Format,
) -> std::result::Result<(), CliError> {
    let report = evaluate_with_receipt(root, number, expected_receipt).map_err(CliError::Usage)?;
    match format {
        Format::Json => println!(
            "{}",
            serde_json::to_string_pretty(&report).map_err(|e| CliError::Verify(e.into()))?
        ),
        Format::Text => print_text(&report),
    }
    match report.authorization.decision {
        AuthorizationDecision::MergeAllowed => Ok(()),
        AuthorizationDecision::Blocked => Err(CliError::Verify(anyhow::anyhow!(
            "PR {number} is not ready to merge ({} blocker(s))",
            report.authorization.reasons.len()
        ))),
        AuthorizationDecision::Unknown => Err(CliError::Usage(anyhow::anyhow!(
            "PR {number} merge authorization is indeterminate ({} reason(s)): {}",
            report.authorization.reasons.len(),
            summarise_reasons(&report.authorization.reasons)
        ))),
    }
}

/// Joins up to three reason codes and messages for a one-line failure.
fn summarise_reasons(reasons: &[AuthorizationReason]) -> String {
    reasons
        .iter()
        .take(3)
        .map(|reason| format!("[{}] {}", reason.code, reason.message))
        .collect::<Vec<_>>()
        .join("; ")
}

fn print_text(report: &ReadinessReport) {
    let headline = if report.ready { "READY" } else { "NOT READY" };
    println!("pr {}: merge readiness: {headline}", report.pr);
    println!(
        "  authorization: {} ({} blocker(s))",
        report.authorization.decision.as_str(),
        report.authorization.reasons.len()
    );
    println!(
        "  merge-state: {} (mergeable: {})",
        report.merge_state.status, report.merge_state.mergeable
    );
    println!(
        "  checks: {} passed, {} cancelled, {} failed, {} pending, {} skipped",
        report.checks.passed,
        report.checks.cancelled.len(),
        report.checks.failed.len(),
        report.checks.pending.len(),
        report.checks.skipped,
    );
    println!("  checks verdict: {}", report.checks.verdict);
    if report.checks.incomplete {
        println!(
            "  checks are INCOMPLETE: merge conflicts stop workflow checks from being scheduled"
        );
    }
    for c in &report.checks.cancelled {
        println!("    CANCELLED: {} -> {}", c.name, c.rerun_command);
    }
    for f in &report.checks.failed {
        println!("    FAIL: {}", f.name);
    }
    for p in &report.checks.pending {
        println!("    PENDING: {p}");
    }
    for u in &report.checks.unknown {
        println!("    UNKNOWN: {u}");
    }

    println!(
        "  conversations: {} unresolved thread(s), {} actionable comment(s), {} needing triage",
        report.conversations.unresolved_threads.len(),
        report.conversations.actionable_comments.len(),
        report.conversations.needs_triage.len()
    );
    for t in &report.conversations.unresolved_threads {
        let loc = t.path.as_deref().unwrap_or("diff");
        let outdated = if t.is_outdated { " (outdated)" } else { "" };
        println!(
            "    UNRESOLVED{outdated}: {loc} by {}: \"{}\"",
            t.author, t.excerpt
        );
    }
    for c in &report.conversations.actionable_comments {
        println!("    ACTIONABLE: {}: \"{}\"", c.author, c.excerpt);
    }
    println!(
        "  findings: {} total ({} introduced, {} regressed, {} pre-existing, {} untracked)",
        report.findings.total,
        report.findings.introduced,
        report.findings.regressed,
        report.findings.pre_existing,
        report.findings.untracked
    );

    if let Some(receipt) = &report.authorization.receipt {
        println!(
            "  receipt: {} (expires at unix {}, snapshot {})",
            receipt.pr, receipt.expires_at_unix, receipt.snapshot_sha256
        );
    }
    if !report.blockers.is_empty() {
        println!("blockers:");
        for blocker in &report.blockers {
            println!("  - {blocker}");
        }
    }
    if !report.authorization.reasons.is_empty() {
        println!("authorization reasons:");
        for reason in &report.authorization.reasons {
            println!("  - [{}] {}", reason.code, reason.message);
        }
    }
}
