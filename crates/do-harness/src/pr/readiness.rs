//! PR merge-readiness analysis: merge state, check buckets, comments inventory, and Codecov digest.

use std::path::Path;

use anyhow::Result;

use super::gh;
pub use super::readiness_types::*;
use crate::report::Format;

/// Executes the merge readiness check for a PR.
///
/// # Errors
///
/// Returns an error if `gh` CLI queries fail or if the PR is not ready to merge.
pub fn run(root: &Path, pr_number: u64, format: Format) -> Result<()> {
    let view = gh::view_extended(root, pr_number)?;
    let check_runs = gh::fetch_check_runs(root, &view.head_ref_oid).unwrap_or_default();
    let commit_statuses = gh::fetch_commit_statuses(root, &view.head_ref_oid).unwrap_or_default();
    let review_comments = gh::fetch_review_comments(root, pr_number).unwrap_or_default();
    let reviews = gh::fetch_reviews(root, pr_number).unwrap_or_default();
    let issue_comments = gh::fetch_issue_comments(root, pr_number).unwrap_or_default();
    let review_threads = gh::fetch_review_threads(root, pr_number).unwrap_or_default();

    let report = analyze_readiness_data(
        &view,
        &check_runs,
        &commit_statuses,
        &review_comments,
        &reviews,
        &issue_comments,
        &review_threads,
    );

    if matches!(format, Format::Json) {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        print_report_text(&report);
    }

    if !report.ready_to_merge {
        anyhow::bail!(
            "PR #{pr_number} is not ready for merge ({} actionable item(s) found)",
            report.actionable_items.len()
        );
    }

    Ok(())
}

/// Analyzes collected PR data to produce a [`ReadinessReport`].
#[must_use]
pub fn analyze_readiness_data(
    view: &gh::PrViewExtended,
    check_runs: &[gh::CheckRunItem],
    commit_statuses: &[gh::CommitStatusItem],
    review_comments: &[gh::ReviewCommentItem],
    reviews: &[gh::PrReviewItem],
    issue_comments: &[gh::IssueCommentItem],
    review_threads: &[gh::GqlThreadItem],
) -> ReadinessReport {
    let mut actionable_items = Vec::new();

    let merge_state = view.merge_state_status.clone();
    if merge_state != "CLEAN" {
        actionable_items.push(ActionableItem {
            kind: "merge_state".to_string(),
            title: format!("Merge state is {merge_state}"),
            description: format!(
                "PR #{} mergeStateStatus is '{merge_state}'; must be CLEAN to merge.",
                view.number
            ),
            rerun_command: None,
            url: Some(view.url.clone()),
        });
    }

    let checks_summary = analyze_checks(check_runs, commit_statuses, &mut actionable_items);
    let comments_summary = analyze_comments(
        review_comments,
        reviews,
        issue_comments,
        review_threads,
        &mut actionable_items,
    );

    let codecov_digest = analyze_codecov(issue_comments, review_comments);
    if let Some(ref digest) = codecov_digest {
        if digest.status == "actionable_gap" {
            actionable_items.push(ActionableItem {
                kind: "codecov_gap".to_string(),
                title: "Codecov actionable coverage gap".to_string(),
                description: digest.summary.clone(),
                rerun_command: None,
                url: digest.comment_url.clone(),
            });
        }
    }

    ReadinessReport {
        schema_version: SCHEMA_VERSION,
        pr: view.number,
        title: view.title.clone(),
        url: view.url.clone(),
        head_sha: view.head_ref_oid.clone(),
        base_branch: view.base_ref_name.clone(),
        merge_state,
        mergeable: view.mergeable,
        checks: checks_summary,
        comments: comments_summary,
        codecov: codecov_digest,
        ready_to_merge: actionable_items.is_empty(),
        actionable_items,
    }
}

#[allow(clippy::too_many_lines)]
fn analyze_checks(
    check_runs: &[gh::CheckRunItem],
    commit_statuses: &[gh::CommitStatusItem],
    actionable_items: &mut Vec<ActionableItem>,
) -> CheckSummary {
    let (mut passed, mut cancelled, mut failed, mut pending, mut skipped) = (0, 0, 0, 0, 0);
    let mut details = Vec::new();

    for run in check_runs {
        let name = run.name.clone();
        let status = run.status.clone();
        let conclusion = run.conclusion.clone().unwrap_or_default();
        let url = run.html_url.clone().or_else(|| run.details_url.clone());
        let bucket = classify_check_run_bucket(&status, &conclusion);
        let rerun_cmd = if bucket == "cancelled" || bucket == "failed" {
            url.as_deref().and_then(extract_rerun_command)
        } else {
            None
        };

        match bucket.as_str() {
            "passed" => passed += 1,
            "cancelled" => {
                cancelled += 1;
                let rerun_str = rerun_cmd
                    .clone()
                    .unwrap_or_else(|| "gh run rerun --failed".to_string());
                actionable_items.push(ActionableItem {
                    kind: "cancelled_check".to_string(),
                    title: format!("Check leg cancelled: {name}"),
                    description: format!("Leg '{name}' was CANCELLED. Rerun command: {rerun_str}"),
                    rerun_command: Some(rerun_str),
                    url: url.clone(),
                });
            }
            "failed" => {
                failed += 1;
                let rerun_str = rerun_cmd
                    .clone()
                    .unwrap_or_else(|| "gh run rerun --failed".to_string());
                actionable_items.push(ActionableItem {
                    kind: "failed_check".to_string(),
                    title: format!("Check failed: {name}"),
                    description: format!(
                        "Check leg '{name}' failed with conclusion '{conclusion}'."
                    ),
                    rerun_command: Some(rerun_str),
                    url: url.clone(),
                });
            }
            "pending" => {
                pending += 1;
                actionable_items.push(ActionableItem {
                    kind: "pending_check".to_string(),
                    title: format!("Check pending: {name}"),
                    description: format!("Check leg '{name}' is still in progress."),
                    rerun_command: None,
                    url: url.clone(),
                });
            }
            _ => skipped += 1,
        }

        details.push(CheckDetail {
            name,
            status,
            conclusion,
            bucket,
            url,
            rerun_command: rerun_cmd,
        });
    }

    for status in commit_statuses {
        let name = status.context.clone();
        let st = status.state.clone();
        let url = status.target_url.clone();
        let bucket = match st.as_str() {
            "success" => "passed",
            "pending" => "pending",
            _ => "failed",
        };
        match bucket {
            "passed" => passed += 1,
            "pending" => {
                pending += 1;
                actionable_items.push(ActionableItem {
                    kind: "pending_check".to_string(),
                    title: format!("Status pending: {name}"),
                    description: format!("Commit status '{name}' is pending."),
                    rerun_command: None,
                    url: url.clone(),
                });
            }
            _ => {
                failed += 1;
                actionable_items.push(ActionableItem {
                    kind: "failed_check".to_string(),
                    title: format!("Status failed: {name}"),
                    description: format!("Commit status '{name}' failed with state '{st}'."),
                    rerun_command: None,
                    url: url.clone(),
                });
            }
        }
        details.push(CheckDetail {
            name,
            status: st.clone(),
            conclusion: st.clone(),
            bucket: bucket.to_string(),
            url,
            rerun_command: None,
        });
    }

    CheckSummary {
        passed,
        cancelled,
        failed,
        pending,
        skipped,
        details,
    }
}

fn analyze_comments(
    review_comments: &[gh::ReviewCommentItem],
    reviews: &[gh::PrReviewItem],
    issue_comments: &[gh::IssueCommentItem],
    review_threads: &[gh::GqlThreadItem],
    actionable_items: &mut Vec<ActionableItem>,
) -> CommentSummary {
    let (mut human_comments_count, mut bot_comments_count) = (0, 0);

    for c in issue_comments {
        if is_bot_user(&c.user.login) {
            bot_comments_count += 1;
        } else {
            human_comments_count += 1;
        }
    }
    for c in review_comments {
        if is_bot_user(&c.user.login) {
            bot_comments_count += 1;
        } else {
            human_comments_count += 1;
        }
    }

    let review_threads_total = review_threads.len();
    let mut review_threads_unresolved = 0;

    for thread in review_threads {
        if !thread.is_resolved {
            review_threads_unresolved += 1;
            let path = if thread.path.is_empty() {
                "file".to_string()
            } else {
                thread.path.clone()
            };
            actionable_items.push(ActionableItem {
                kind: "unresolved_thread".to_string(),
                title: format!("Unresolved thread on {path}"),
                description: format!(
                    "Unresolved review thread by {} on {path}:{}",
                    thread.author, thread.line
                ),
                rerun_command: None,
                url: None,
            });
        }
    }

    for rev in reviews {
        if rev.state == "CHANGES_REQUESTED" {
            actionable_items.push(ActionableItem {
                kind: "changes_requested".to_string(),
                title: format!("Changes requested by {}", rev.user.login),
                description: format!("Reviewer {} requested changes on PR.", rev.user.login),
                rerun_command: None,
                url: None,
            });
        }
    }

    CommentSummary {
        human_comments_count,
        bot_comments_count,
        review_threads_total,
        review_threads_unresolved,
    }
}

fn classify_check_run_bucket(status: &str, conclusion: &str) -> String {
    if status != "completed" {
        return "pending".to_string();
    }
    match conclusion {
        "success" => "passed".to_string(),
        "cancelled" | "cancel" => "cancelled".to_string(),
        "skipped" | "neutral" => "skipped".to_string(),
        _ => "failed".to_string(),
    }
}

fn is_bot_user(login: &str) -> bool {
    login.ends_with("[bot]")
        || login.contains("codecov")
        || login == "dependabot"
        || login == "renovate"
}

/// Extracts `gh run rerun <run_id> --failed` command from a check-run or job URL.
#[must_use]
pub fn extract_rerun_command(url: &str) -> Option<String> {
    let marker = "/actions/runs/";
    if let Some(pos) = url.find(marker) {
        let rest = &url[pos + marker.len()..];
        let run_id: String = rest.chars().take_while(char::is_ascii_digit).collect();
        if !run_id.is_empty() {
            return Some(format!("gh run rerun {run_id} --failed"));
        }
    }
    None
}

fn analyze_codecov(
    issue_comments: &[gh::IssueCommentItem],
    review_comments: &[gh::ReviewCommentItem],
) -> Option<CodecovDigest> {
    let mut body = None;
    let mut url = None;

    for ic in issue_comments.iter().rev() {
        if is_codecov_comment(&ic.user.login, &ic.body) {
            body = Some(ic.body.clone());
            url.clone_from(&ic.html_url);
            break;
        }
    }
    if body.is_none() {
        for rc in review_comments.iter().rev() {
            if is_codecov_comment(&rc.user.login, &rc.body) {
                body = Some(rc.body.clone());
                url.clone_from(&rc.html_url);
                break;
            }
        }
    }

    let body_str = body?;
    let patch_coverage = extract_patch_coverage(&body_str);
    let is_gap = has_coverage_gap(&body_str, patch_coverage.as_deref());
    let status = if is_gap {
        "actionable_gap".to_string()
    } else {
        "ok".to_string()
    };
    let patch_str = patch_coverage.as_deref().unwrap_or("unknown");
    let summary = if is_gap {
        format!("Codecov reported an actionable coverage gap (patch coverage: {patch_str}).")
    } else {
        format!("Codecov report passed (patch coverage: {patch_str}).")
    };

    Some(CodecovDigest {
        present: true,
        patch_coverage,
        status,
        summary,
        comment_url: url,
    })
}

fn is_codecov_comment(author: &str, body: &str) -> bool {
    author.contains("codecov")
        || body.contains("Codecov Report")
        || body.contains("Patch Coverage")
        || body.contains("Coverage Diff")
}

fn extract_patch_coverage(body: &str) -> Option<String> {
    for line in body.lines() {
        if line.to_lowercase().contains("patch") || line.contains("diff") {
            if let Some(pct) = find_percentage_in_str(line) {
                return Some(pct);
            }
        }
    }
    find_percentage_in_str(body)
}

fn find_percentage_in_str(s: &str) -> Option<String> {
    let chars: Vec<char> = s.chars().collect();
    for i in 0..chars.len() {
        if chars[i] == '%' {
            let mut j = i;
            while j > 0 && (chars[j - 1].is_ascii_digit() || chars[j - 1] == '.') {
                j -= 1;
            }
            if j < i {
                return Some(chars[j..=i].iter().collect());
            }
        }
    }
    None
}

fn has_coverage_gap(body: &str, patch_coverage: Option<&str>) -> bool {
    if let Some(patch) = patch_coverage {
        let clean = patch.trim_end_matches('%');
        if let Ok(val) = clean.parse::<f64>() {
            if val < 100.0 {
                return true;
            }
        }
    }
    let lower = body.to_lowercase();
    lower.contains("missing lines")
        || lower.contains("uncovered lines")
        || lower.contains("0.00% of diff")
        || lower.contains("0% of diff")
        || lower.contains("coverage drop")
        || lower.contains("target failed")
}

fn print_report_text(report: &ReadinessReport) {
    let status_str = if report.ready_to_merge {
        "READY TO MERGE"
    } else {
        "NOT READY TO MERGE"
    };
    println!("PR #{}: {} ({})", report.pr, report.title, status_str);
    println!("URL: {}", report.url);
    println!("Base: {} | Head: {}", report.base_branch, report.head_sha);
    println!(
        "Merge State: {} (mergeable: {})",
        report.merge_state, report.mergeable
    );

    let c = &report.checks;
    println!(
        "Checks: {} passed, {} cancelled, {} failed, {} pending, {} skipped",
        c.passed, c.cancelled, c.failed, c.pending, c.skipped
    );

    let cm = &report.comments;
    println!(
        "Comments: {} human, {} bot | Review Threads: {} total, {} unresolved",
        cm.human_comments_count,
        cm.bot_comments_count,
        cm.review_threads_total,
        cm.review_threads_unresolved
    );

    if let Some(ref cc) = report.codecov {
        println!(
            "Codecov: {} (patch: {})",
            cc.status,
            cc.patch_coverage.as_deref().unwrap_or("N/A")
        );
    } else {
        println!("Codecov: none found");
    }

    if !report.actionable_items.is_empty() {
        println!("\nActionable items ({}):", report.actionable_items.len());
        for item in &report.actionable_items {
            println!("  - [{}] {}", item.kind, item.title);
            println!("    {}", item.description);
            if let Some(ref cmd) = item.rerun_command {
                println!("    Rerun command: {cmd}");
            }
        }
    }
}

#[cfg(test)]
#[path = "readiness_tests.rs"]
mod readiness_tests;
