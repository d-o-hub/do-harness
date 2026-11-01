//! Cross-platform coverage of the merge-gating classification.
//!
//! The end-to-end `pr ready` suites drive a fake `gh` shell script and are
//! therefore `#[cfg(unix)]`. These cases call the pure fold directly, so the
//! logic that decides whether a pull request may merge is exercised on Windows
//! too.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;

fn run(name: &str, status: &str, conclusion: Option<&str>) -> CheckRun {
    CheckRun {
        id: None,
        name: name.to_owned(),
        status: status.to_owned(),
        conclusion: conclusion.map(str::to_owned),
        html_url: None,
        started_at: None,
        completed_at: None,
    }
}

fn status(context: &str, state: &str) -> CommitStatus {
    CommitStatus {
        context: context.to_owned(),
        state: state.to_owned(),
        target_url: None,
    }
}

fn summarize(
    runs: Vec<CheckRun>,
    statuses: Vec<CommitStatus>,
    conflicts: bool,
) -> (CheckSummary, Vec<String>) {
    let mut blockers = Vec::new();
    let summary = summarize_checks(runs, statuses, conflicts, &mut blockers);
    (summary, blockers)
}

#[test]
fn a_pending_run_blocks_and_is_counted() {
    let (summary, blockers) = summarize(vec![run("build", "in_progress", None)], vec![], false);
    assert_eq!(summary.total, 1);
    assert_eq!(summary.pending, vec!["build".to_owned()]);
    assert!(blockers.iter().any(|b| b.contains("still pending")));
    assert!(!summary.incomplete);
}

#[test]
fn a_failed_run_blocks_and_is_counted_as_failed() {
    let (summary, blockers) = summarize(
        vec![run("test", "completed", Some("failure"))],
        vec![],
        false,
    );
    assert_eq!(summary.failed.len(), 1);
    assert_eq!(summary.failed[0].name, "test");
    assert!(blockers.iter().any(|b| b.contains("failed (failure)")));
}

#[test]
fn a_cancelled_run_carries_a_rerun_command() {
    let (summary, blockers) = summarize(
        vec![run("lint", "completed", Some("cancelled"))],
        vec![],
        false,
    );
    assert_eq!(summary.cancelled.len(), 1);
    assert!(
        summary.cancelled[0]
            .rerun_command
            .starts_with("gh run rerun")
    );
    assert!(blockers.iter().any(|b| b.contains("CANCELLED")));
}

#[test]
fn success_and_neutral_runs_do_not_block() {
    let (summary, blockers) = summarize(
        vec![
            run("build", "completed", Some("success")),
            run("docs", "completed", Some("skipped")),
            run("lint", "completed", Some("neutral")),
        ],
        vec![],
        false,
    );
    assert_eq!(summary.passed, 1);
    assert_eq!(summary.skipped, 2);
    assert!(blockers.is_empty(), "unexpected blockers: {blockers:?}");
}

#[test]
fn a_completed_run_without_conclusion_blocks() {
    let (summary, blockers) = summarize(vec![run("test", "completed", None)], vec![], false);
    assert_eq!(summary.failed.len(), 1);
    assert!(blockers.iter().any(|b| b.contains("without conclusion")));
}

#[test]
fn commit_statuses_are_classified_by_context() {
    let statuses = vec![
        status("ci/ok", "success"),
        status("ci/wait", "pending"),
        status("ci/bad", "error"),
    ];
    let expected_total = statuses.len();
    let (summary, blockers) = summarize(vec![], statuses, false);
    assert_eq!(summary.passed, 1);
    assert_eq!(summary.pending, vec!["ci/wait".to_owned()]);
    assert_eq!(summary.failed.len(), 1);
    assert_eq!(summary.failed[0].name, "ci/bad");
    assert_eq!(summary.total, expected_total);
    assert!(
        blockers
            .iter()
            .any(|b| b.contains("status 'ci/wait' is pending"))
    );
}

#[test]
fn conflicts_mark_a_non_empty_run_set_incomplete() {
    let (summary, blockers) = summarize(
        vec![run("build", "completed", Some("success"))],
        vec![],
        true,
    );
    assert!(summary.incomplete);
    assert!(blockers.iter().any(|b| b.contains("checks are incomplete")));
}

#[test]
fn conflicts_with_no_reported_checks_do_not_claim_incompleteness() {
    let (summary, blockers) = summarize(vec![], vec![], true);
    assert!(!summary.incomplete);
    assert!(blockers.is_empty(), "unexpected blockers: {blockers:?}");
}
