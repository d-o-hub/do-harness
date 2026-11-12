//! Collection helpers for the readiness snapshot.
//!
//! Each helper pushes human-readable blockers and returns its own projection,
//! so [`super::evaluate_with_receipt`] only sequences them and seals the
//! authorization.

use std::path::Path;

use anyhow::{Context, Result};

use super::model::{
    CancelledCheck, CheckSummary, ConversationSummary, FailedCheck, FindingsSummary,
    UnresolvedThread,
};
use crate::pr::checks;
use crate::pr::conversations;
use crate::pr::findings::{self, FindingClassification, FindingSeverity};
use crate::pr::gh;
use crate::pr::waiver_comment::first_line;

/// Classifies CI state through the typed checker and folds its blockers in.
///
/// `base` comes from the view the snapshot already read, so the required set is
/// resolved against the same base the authorization will use — reading it twice
/// could consult a different ruleset if the base changed between calls.
pub(super) fn evaluate_checks(
    root: &Path,
    pr: u64,
    head_ref_oid: &str,
    conflicts: bool,
    base: &str,
    blockers: &mut Vec<String>,
) -> Result<CheckSummary> {
    // Fail closed: a `gh` failure means the check state is unknown, not empty.
    let report = checks::evaluate_for_head_with_base(root, pr, head_ref_oid, conflicts, base)?;
    for blocker in &report.blockers {
        blockers.push(blocker.message.clone());
    }
    Ok(summarize_report(report))
}

/// Projects the typed CI report into the readiness summary shape.
fn summarize_report(report: checks::ChecksReport) -> CheckSummary {
    let mut passed = 0;
    let mut skipped = 0;
    let mut pending = Vec::new();
    let mut failed = Vec::new();
    let mut cancelled = Vec::new();
    let mut unknown = Vec::new();
    for entry in &report.checks {
        match entry.verdict {
            checks::Verdict::Pass => passed += 1,
            checks::Verdict::Skip => skipped += 1,
            checks::Verdict::Pending => pending.push(entry.name.clone()),
            checks::Verdict::Fail => {
                if entry.conclusion.as_deref() == Some("cancelled") {
                    cancelled.push(CancelledCheck {
                        name: entry.name.clone(),
                        rerun_command: entry.rerun_command.clone().unwrap_or_default(),
                        url: entry.url.clone(),
                    });
                } else {
                    failed.push(FailedCheck {
                        name: entry.name.clone(),
                        url: entry.url.clone(),
                    });
                }
            }
            checks::Verdict::Unknown => unknown.push(entry.name.clone()),
        }
    }
    let total = report.checks.len();
    let incomplete = report
        .blockers
        .iter()
        .any(|blocker| blocker.code == "checks_incomplete");
    CheckSummary {
        total,
        passed,
        failed,
        cancelled,
        pending,
        skipped,
        incomplete,
        verdict: report.verdict.as_str().to_owned(),
        head_sha: report.head_sha,
        unknown,
        entries: report.checks,
        blockers: report.blockers,
    }
}

/// Builds the complete conversation ledger and records its blockers.
pub(super) fn evaluate_conversations(
    root: &Path,
    number: u64,
    view: &gh::PrReadinessView,
    checks: &CheckSummary,
    blockers: &mut Vec<String>,
) -> Result<(ConversationSummary, Option<super::CodecovSummary>)> {
    let threads = gh::review_threads(root, number).context("could not read review threads")?;
    let comments = gh::issue_comments(root, number).context("could not read issue comments")?;
    let reviews = gh::reviews(root, number).context("could not read reviews")?;

    // The classified entries are already collected, and an app can only be
    // attributed by a check it published on *this* head, so they are passed in
    // rather than re-read (#517).
    let ledger = conversations::build_inventory(
        root,
        number,
        view,
        &threads,
        &comments,
        &reviews,
        &checks.entries,
    )?;

    let mut unresolved_threads = Vec::new();
    for thread in &threads {
        if !thread.is_resolved {
            let author = thread
                .comments
                .nodes
                .first()
                .and_then(|c| c.author.as_ref())
                .map_or("unknown", |a| a.login.as_str());
            let excerpt = thread
                .comments
                .nodes
                .first()
                .map(|c| first_line(&c.body, 80))
                .unwrap_or_default();
            let location = match (&thread.path, thread.line) {
                (Some(p), Some(l)) => format!("{p}:{l}"),
                (Some(p), None) => p.clone(),
                _ => "diff".to_owned(),
            };
            let status_note = if thread.is_outdated {
                " (outdated)"
            } else {
                ""
            };
            blockers.push(format!(
                "unresolved review thread{status_note} on {location} by {author}: {excerpt}"
            ));
            unresolved_threads.push(UnresolvedThread {
                id: thread.id.clone(),
                path: thread.path.clone(),
                line: thread.line,
                author: author.to_owned(),
                excerpt,
                is_outdated: thread.is_outdated,
            });
        }
    }

    let mut changes_requested_reviews = 0;
    for review in &reviews {
        if review.state == "CHANGES_REQUESTED" {
            changes_requested_reviews += 1;
            let author = review.user.as_ref().map_or("unknown", |u| u.login.as_str());
            let item_ref = format!(
                "review:{}",
                review.node_id.as_deref().unwrap_or(&review.id.to_string())
            );
            if !ledger
                .accepted_dispositions
                .iter()
                .any(|d| d.item_ref == item_ref)
            {
                blockers.push(format!(
                    "review by {author} requested changes: {}",
                    first_line(&review.body, 80)
                ));
            }
        }
    }

    for item in &ledger.needs_triage {
        blockers.push(format!(
            "conversation item '{}' by {} needs triage: {}",
            item.item_ref, item.author, item.excerpt
        ));
    }

    let mut actionable_comments = Vec::new();
    let codecov = crate::pr::waiver_comment::evaluate_codecov(
        root,
        &comments,
        &view.head_ref_oid,
        blockers,
        &mut actionable_comments,
    );

    let summary = ConversationSummary {
        unresolved_threads,
        actionable_comments,
        review_decision: view.review_decision.clone(),
        conversation_inventory_digest: ledger.inventory_digest,
        items_count: ledger.items.len(),
        needs_triage: ledger.needs_triage,
        items: ledger.items,
        changes_requested_reviews,
        complete: ledger.complete,
    };
    Ok((summary, codecov))
}

/// Evaluates findings provenance and records merge blockers.
///
/// Introduced and regressed findings always block. A critical finding blocks
/// until it is linked to a tracked issue or carries an authorized disposition.
/// A failed collection blocks too: an unreadable finding set is not a clean one.
pub(super) fn evaluate_findings_block(
    root: &Path,
    view: &gh::PrReadinessView,
    blockers: &mut Vec<String>,
) -> FindingsSummary {
    let collected =
        findings::collect_and_classify(root, &view.head_ref_oid, &view.base_ref_oid, "repository");

    let (collected, base_verified) = match collected {
        Ok(pair) => pair,
        Err(err) => {
            blockers.push(format!("findings collection failed: {err:#}"));
            return FindingsSummary::default();
        }
    };

    let mut summary = FindingsSummary {
        total: collected.len(),
        base_verified,
        digest: findings::compute_findings_digest(&collected),
        ..FindingsSummary::default()
    };

    for finding in &collected {
        let location = match finding.start_line {
            Some(line) => format!("{}:{}", finding.path, line),
            None => finding.path.clone(),
        };
        match finding.classification {
            FindingClassification::Introduced => {
                summary.introduced += 1;
                blockers.push(format!(
                    "introduced {} finding '{}' at {location}: {}",
                    finding.severity.as_str(),
                    finding.rule_id,
                    finding.message
                ));
            }
            FindingClassification::Regressed => {
                summary.regressed += 1;
                blockers.push(format!(
                    "regressed {} finding '{}' at {location}: {}",
                    finding.severity.as_str(),
                    finding.rule_id,
                    finding.message
                ));
            }
            FindingClassification::PreExisting => summary.pre_existing += 1,
            FindingClassification::Resolved | FindingClassification::Unknown => {}
        }

        if finding.issue_url.is_none() && finding.disposition.is_none() {
            summary.untracked += 1;
            if finding.severity == FindingSeverity::Critical {
                blockers.push(format!(
                    "critical finding '{}' at {location} is untracked (no linked issue, no disposition)",
                    finding.rule_id
                ));
            }
        }
    }

    summary
}

/// Re-exported fold for the cross-platform classification tests.
#[cfg(test)]
pub(super) fn summarize_report_for_tests(report: checks::ChecksReport) -> CheckSummary {
    summarize_report(report)
}
