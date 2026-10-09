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
use serde::{Deserialize, Serialize};

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

/// Merge readiness evaluation report.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReadinessReport {
    pub pr: u64,
    pub ready: bool,
    pub merge_state: MergeState,
    pub checks: CheckSummary,
    pub conversations: ConversationSummary,
    pub codecov: Option<CodecovSummary>,
    pub blockers: Vec<String>,
}

/// PR mergeability state from `GitHub`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MergeState {
    pub status: String,
    pub mergeable: String,
    pub auto_merge: bool,
}

/// Categorized CI checks and commit statuses.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckSummary {
    pub total: usize,
    pub passed: usize,
    pub failed: Vec<FailedCheck>,
    pub cancelled: Vec<CancelledCheck>,
    pub pending: Vec<String>,
    pub skipped: usize,
}

/// A failed check run or commit status.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailedCheck {
    pub name: String,
    pub url: Option<String>,
}

/// A cancelled check run with exact rerun command.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CancelledCheck {
    pub name: String,
    pub rerun_command: String,
    pub url: Option<String>,
}

/// Review threads and actionable comments.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationSummary {
    pub unresolved_threads: Vec<UnresolvedThread>,
    pub actionable_comments: Vec<ActionableComment>,
}

/// An unresolved review thread.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnresolvedThread {
    pub id: String,
    pub path: Option<String>,
    pub line: Option<u64>,
    pub author: String,
    pub excerpt: String,
}

/// An actionable top-level comment (e.g. Codecov gap).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionableComment {
    pub id: Option<u64>,
    pub author: String,
    pub excerpt: String,
    pub url: Option<String>,
}


/// Evaluates merge readiness for PR `number`.
///
/// # Errors
///
/// Returns an error when querying `GitHub` fails.
pub fn evaluate(root: &Path, number: u64) -> Result<ReadinessReport> {
    let view = gh::readiness_view(root, number)?;
    let mut blockers = Vec::new();

    // 1. Merge State
    let is_clean = view.merge_state_status == "CLEAN";
    let is_mergeable = view.mergeable == "MERGEABLE";
    if !is_clean {
        blockers.push(format!(
            "merge state is '{}' (expected 'CLEAN')",
            view.merge_state_status
        ));
    }
    if !is_mergeable {
        blockers.push(format!(
            "mergeability is '{}' (expected 'MERGEABLE')",
            view.mergeable
        ));
    }
    let merge_state = MergeState {
        status: view.merge_state_status,
        mergeable: view.mergeable,
        auto_merge: view.auto_merge_request.is_some(),
    };

    // 2. Checks & Statuses
    let checks = evaluate_checks(root, &view.head_ref_oid, &mut blockers)?;

    // 3. Review Threads & Issue Comments
    let (conversations, codecov) =
        evaluate_conversations(root, number, &view.head_ref_oid, &mut blockers)?;

    let ready = blockers.is_empty();
    Ok(ReadinessReport {
        pr: number,
        ready,
        merge_state,
        checks,
        conversations,
        codecov,
        blockers,
    })
}

fn evaluate_checks(
    root: &Path,
    head_ref_oid: &str,
    blockers: &mut Vec<String>,
) -> Result<CheckSummary> {
    // Fail closed: a `gh` failure means the check state is unknown, not empty.
    let check_runs = gh::check_runs(root, head_ref_oid).context("could not read check runs")?;
    let statuses =
        gh::commit_statuses(root, head_ref_oid).context("could not read commit statuses")?;

    let mut passed = 0;
    let mut skipped = 0;
    let mut pending = Vec::new();
    let mut failed = Vec::new();
    let mut cancelled = Vec::new();

    for run in check_runs {
        if run.status != "completed" {
            blockers.push(format!(
                "check '{}' is still pending ({})",
                run.name, run.status
            ));
            pending.push(run.name);
            continue;
        }
        match run.conclusion.as_deref() {
            Some("success") => passed += 1,
            Some("skipped" | "neutral") => skipped += 1,
            Some("cancelled") => {
                let rerun_command = run.rerun_command();
                blockers.push(format!(
                    "check '{}' was CANCELLED; rerun with: {rerun_command}",
                    run.name
                ));
                cancelled.push(CancelledCheck {
                    name: run.name,
                    rerun_command,
                    url: run.html_url,
                });
            }
            Some(other) => {
                blockers.push(format!("check '{}' failed ({other})", run.name));
                failed.push(FailedCheck {
                    name: run.name,
                    url: run.html_url,
                });
            }
            None => {
                blockers.push(format!("check '{}' completed without conclusion", run.name));
                failed.push(FailedCheck {
                    name: run.name,
                    url: run.html_url,
                });
            }
        }
    }

    for status in statuses {
        match status.state.as_str() {
            "success" => passed += 1,
            "pending" => {
                blockers.push(format!("status '{}' is pending", status.context));
                pending.push(status.context);
            }
            other => {
                blockers.push(format!("status '{}' failed ({other})", status.context));
                failed.push(FailedCheck {
                    name: status.context,
                    url: status.target_url,
                });
            }
        }
    }

    let total = passed + skipped + pending.len() + failed.len() + cancelled.len();
    Ok(CheckSummary {
        total,
        passed,
        failed,
        cancelled,
        pending,
        skipped,
    })
}

fn evaluate_conversations(
    root: &Path,
    number: u64,
    head_ref_oid: &str,
    blockers: &mut Vec<String>,
) -> Result<(ConversationSummary, Option<CodecovSummary>)> {
    let threads = gh::review_threads(root, number).context("could not read review threads")?;
    let mut unresolved_threads = Vec::new();
    for thread in threads {
        if !thread.is_resolved && !thread.is_outdated {
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
            blockers.push(format!(
                "unresolved review thread on {location} by {author}: {excerpt}"
            ));
            unresolved_threads.push(UnresolvedThread {
                id: thread.id,
                path: thread.path,
                line: thread.line,
                author: author.to_owned(),
                excerpt,
            });
        }
    }

    let comments = gh::issue_comments(root, number).context("could not read issue comments")?;
    let mut actionable_comments = Vec::new();
    let codecov = super::waiver_comment::evaluate_codecov(
        root,
        &comments,
        head_ref_oid,
        blockers,
        &mut actionable_comments,
    );

    let conversations = ConversationSummary {
        unresolved_threads,
        actionable_comments,
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
    for c in &report.checks.cancelled {
        println!("    CANCELLED: {} -> {}", c.name, c.rerun_command);
    }
    for f in &report.checks.failed {
        println!("    FAIL: {}", f.name);
    }
    for p in &report.checks.pending {
        println!("    PENDING: {p}");
    }

    println!(
        "  conversations: {} unresolved thread(s), {} actionable comment(s)",
        report.conversations.unresolved_threads.len(),
        report.conversations.actionable_comments.len()
    );
    for t in &report.conversations.unresolved_threads {
        let loc = t.path.as_deref().unwrap_or("diff");
        println!("    UNRESOLVED: {loc} by {}: \"{}\"", t.author, t.excerpt);
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
