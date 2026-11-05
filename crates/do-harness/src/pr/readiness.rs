//! Deterministic PR merge readiness inspection (#240).
//!
//! Evaluates:
//! - Merge state (`mergeStateStatus`, `mergeable`)
//! - CI check runs and commit statuses, categorizing `cancelled` separately
//!   from `failure`, with exact `gh run rerun` reproduction guidance
//! - Review threads and issue comments, detecting unresolved threads and
//!   unanswered actionable comments (e.g. Codecov coverage gaps)

use std::path::Path;

use anyhow::{Context, Result};

use super::checks;
use super::gh;
use super::waiver_comment::first_line;
#[allow(unused_imports)]
pub use super::waiver_comment::{CodecovStatus, CodecovSummary, WaiverAuditRecord};
use crate::CliError;
use crate::report::Format;

/// Logins that post Codecov's own report comments: the app (`codecov[bot]`),
/// the legacy bot user (`codecov`), and the comment-bot account.
const CODECOV_AUTHORS: [&str; 3] = ["codecov", "codecov[bot]", "codecov-commenter"];

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

mod model;
pub use model::{
    ActionableComment, CancelledCheck, CheckSummary, ConversationSummary, FailedCheck,
    FindingsSummary, MergeState, ReadinessReport, UnresolvedThread,
};

/// Evaluates merge readiness for PR `number`.
///
/// # Errors
///
/// Returns an error when querying `GitHub` fails.
pub fn evaluate(root: &Path, number: u64) -> Result<ReadinessReport> {
    const MAX_HEAD_RETRIES: usize = 3;

    for attempt in 0..MAX_HEAD_RETRIES {
        let view = gh::readiness_view(root, number)?;
        let initial_head = view.head_ref_oid.clone();
        let mut blockers = Vec::new();

        // 1. Merge State
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
        // A conflict stops workflow scheduling, so the check set is a subset of
        // the suite; record that before the state strings move into the report.
        let conflicts = view.merge_state_status == "DIRTY" || view.mergeable == "CONFLICTING";
        let merge_state = MergeState {
            status: view.merge_state_status.clone(),
            mergeable: view.mergeable.clone(),
            auto_merge: view.auto_merge_request.is_some(),
        };
        // 2. Review Decision
        if let Some(decision) = &view.review_decision {
            if decision == "CHANGES_REQUESTED" {
                blockers.push("review decision is 'CHANGES_REQUESTED'".to_owned());
            } else if decision == "REVIEW_REQUIRED" {
                blockers.push("review decision is 'REVIEW_REQUIRED'".to_owned());
            }
        }

        // 3. Checks & Statuses
        let checks = evaluate_checks(root, number, &initial_head, conflicts, &mut blockers)?;

        // 4. Review Threads, Comments & Dispositions
        let (conversations, codecov) = evaluate_conversations(root, number, &view, &mut blockers)?;

        // 5. Findings provenance against the verified base baseline
        let findings = evaluate_findings_block(root, &view, &mut blockers);

        // 6. Recheck head SHA after collection
        let recheck_view = gh::readiness_view(root, number)
            .context("failed to recheck head SHA after metadata collection")?;

        if recheck_view.head_ref_oid != initial_head {
            if attempt + 1 < MAX_HEAD_RETRIES {
                continue;
            }
            anyhow::bail!(
                "head SHA moved during inspection from {initial_head} to {}; snapshot is stale/mixed",
                recheck_view.head_ref_oid
            );
        }

        let ready = blockers.is_empty() && checks.verdict == "PASS";
        return Ok(ReadinessReport {
            pr: number,
            head_ref_oid: initial_head,
            collection_complete: conversations.complete,
            ready,
            merge_state,
            checks,
            conversations,
            codecov,
            findings,
            blockers,
        });
    }

    anyhow::bail!("failed to obtain a stable head SHA snapshot after {MAX_HEAD_RETRIES} attempts")
}

/// Evaluates findings provenance and records merge blockers.
///
/// Introduced and regressed findings always block. A critical finding blocks
/// until it is linked to a tracked issue or carries an authorized disposition.
/// A failed collection blocks too: an unreadable finding set is not a clean one.
fn evaluate_findings_block(
    root: &Path,
    view: &gh::PrReadinessView,
    blockers: &mut Vec<String>,
) -> FindingsSummary {
    use super::findings::{FindingClassification, FindingSeverity};

    let collected = super::findings::collect_and_classify(
        root,
        &view.head_ref_oid,
        &view.base_ref_oid,
        "repository",
    );

    let (findings, base_verified) = match collected {
        Ok(pair) => pair,
        Err(err) => {
            blockers.push(format!("findings collection failed: {err:#}"));
            return FindingsSummary::default();
        }
    };

    let mut summary = FindingsSummary {
        total: findings.len(),
        base_verified,
        digest: super::findings::compute_findings_digest(&findings),
        ..FindingsSummary::default()
    };

    for finding in &findings {
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

fn evaluate_checks(
    root: &Path,
    pr: u64,
    head_ref_oid: &str,
    conflicts: bool,
    blockers: &mut Vec<String>,
) -> Result<CheckSummary> {
    // Fail closed: a `gh` failure means the check state is unknown, not empty.
    let report = checks::evaluate_for_head(root, pr, head_ref_oid, conflicts)?;
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

fn evaluate_conversations(
    root: &Path,
    number: u64,
    view: &gh::PrReadinessView,
    blockers: &mut Vec<String>,
) -> Result<(ConversationSummary, Option<CodecovSummary>)> {
    let threads = gh::review_threads(root, number).context("could not read review threads")?;
    let comments = gh::issue_comments(root, number).context("could not read issue comments")?;
    let reviews = gh::reviews(root, number).context("could not read reviews")?;

    let ledger =
        super::conversations::build_inventory(root, number, view, &threads, &comments, &reviews)?;

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

    for rev in &reviews {
        if rev.state == "CHANGES_REQUESTED" {
            let author = rev.user.as_ref().map_or("unknown", |u| u.login.as_str());
            let item_ref = format!(
                "review:{}",
                rev.node_id.as_deref().unwrap_or(&rev.id.to_string())
            );
            if !ledger
                .accepted_dispositions
                .iter()
                .any(|d| d.item_ref == item_ref)
            {
                blockers.push(format!(
                    "review by {author} requested changes: {}",
                    first_line(&rev.body, 80)
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
    let codecov = super::waiver_comment::evaluate_codecov(
        root,
        &comments,
        &view.head_ref_oid,
        blockers,
        &mut actionable_comments,
    );

    let conversations = ConversationSummary {
        unresolved_threads,
        actionable_comments,
        review_decision: view.review_decision.clone(),
        conversation_inventory_digest: ledger.inventory_digest,
        items_count: ledger.items.len(),
        needs_triage: ledger.needs_triage,
        complete: ledger.complete,
    };
    Ok((conversations, codecov))
}

/// Runs the ready command and prints the report in `format`.
///
/// Returns `Ok(())` when ready, or an error when blockers exist.
///
/// # Errors
///
/// Returns [`CliError::Usage`] (exit 2) when `gh` cannot be read — unknown
/// check state must never read as a silent pass — and [`CliError::Verify`]
/// (exit 1) when blockers prevent merging.
pub fn run(root: &Path, number: u64, format: Format) -> std::result::Result<(), CliError> {
    let report = evaluate(root, number).map_err(CliError::Usage)?;
    match format {
        Format::Json => println!(
            "{}",
            serde_json::to_string_pretty(&report).map_err(|e| CliError::Verify(e.into()))?
        ),
        Format::Text => print_text(&report),
    }
    if report.ready {
        Ok(())
    } else {
        Err(CliError::Verify(anyhow::anyhow!(
            "PR {number} is not ready to merge ({} blocker(s))",
            report.blockers.len()
        )))
    }
}

fn print_text(report: &ReadinessReport) {
    if report.ready {
        println!("pr {}: merge readiness: READY", report.pr);
    } else {
        println!(
            "pr {}: merge readiness: NOT READY ({} blocker(s))",
            report.pr,
            report.blockers.len()
        );
    }
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
        "  conversations: {} unresolved thread(s), {} actionable comment(s)",
        report.conversations.unresolved_threads.len(),
        report.conversations.actionable_comments.len()
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

    if let Some(c) = &report.codecov {
        println!(
            "  codecov status: {:?} (actionable: {}, waived: {})",
            c.status, c.is_actionable, c.waived
        );
        if c.waived {
            if let (Some(author), Some(sha), Some(finding), Some(reason)) = (
                &c.waiver_author,
                &c.waiver_head_sha,
                &c.waiver_finding_ref,
                &c.waiver_reason,
            ) {
                println!("    WAIVED: {finding} for {sha} by {author}: \"{reason}\"");
            }
        }
    }

    if !report.blockers.is_empty() {
        println!("blockers:");
        for b in &report.blockers {
            println!("  - {b}");
        }
    }
}
