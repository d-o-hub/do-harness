//! Cross-platform coverage of the merge-gating classification.
//!
//! The end-to-end `pr ready` suites drive a fake `gh` shell script and are
//! therefore `#[cfg(unix)]`. These cases call the pure fold directly, so the
//! logic that decides whether a pull request may merge is exercised on Windows
//! too.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::collect::summarize_report_for_tests as summarize_report;
use super::*;
use crate::pr::checks;
use crate::pr::gh::{CheckRun, CommitStatus};

fn run(name: &str, status: &str, conclusion: Option<&str>) -> CheckRun {
    CheckRun {
        id: None,
        name: name.to_owned(),
        status: status.to_owned(),
        conclusion: conclusion.map(str::to_owned),
        html_url: None,
        app: None,
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
    summarize_with(runs, statuses, conflicts, &checks::ChecksPolicy::default())
}

/// A policy whose required set is an explicit operator decision.
///
/// `ChecksPolicy::default()` deliberately leaves `required_known` false — "the
/// rules were never read" — so a test that means "nothing is required" has to
/// say so instead of inheriting the undetermined state.
fn known_policy(required: &[&str], allow_skips: &[&str]) -> checks::ChecksPolicy {
    let mut policy = checks::ChecksPolicy {
        required: required.iter().map(|s| (*s).to_owned()).collect(),
        required_known: true,
        ..checks::ChecksPolicy::default()
    };
    policy
        .allow_skips
        .extend(allow_skips.iter().map(|s| (*s).to_owned()));
    policy
}

fn summarize_with(
    runs: Vec<CheckRun>,
    statuses: Vec<CommitStatus>,
    conflicts: bool,
    policy: &checks::ChecksPolicy,
) -> (CheckSummary, Vec<String>) {
    let report = checks::classify(1, "head", runs, statuses, policy, conflicts);
    let blockers = report
        .blockers
        .iter()
        .map(|blocker| blocker.message.clone())
        .collect();
    (summarize_report(report), blockers)
}

#[test]
fn a_pending_run_blocks_and_is_counted() {
    let (summary, blockers) = summarize(vec![run("build", "in_progress", None)], vec![], false);
    assert_eq!(summary.total, 1);
    assert_eq!(summary.pending, vec!["build".to_owned()]);
    assert!(blockers.iter().any(|b| b.contains("is pending")));
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
    assert!(blockers.iter().any(|b| b.contains("cancelled")));
}

#[test]
fn success_and_neutral_runs_do_not_block() {
    let mut policy = known_policy(&["build"], &["docs", "lint"]);
    policy.allow_skips.insert("docs".to_owned());
    policy.allow_skips.insert("lint".to_owned());
    let (summary, blockers) = summarize_with(
        vec![
            run("build", "completed", Some("success")),
            run("docs", "completed", Some("skipped")),
            run("lint", "completed", Some("neutral")),
        ],
        vec![],
        false,
        &policy,
    );
    assert_eq!(summary.passed, 1);
    assert_eq!(summary.skipped, 2);
    assert!(blockers.is_empty(), "unexpected blockers: {blockers:?}");
}

/// A skip on a context the policy does not require passes, and is still
/// counted as skipped rather than silently dropped (#516).
#[test]
fn an_optional_skip_does_not_block() {
    let policy = known_policy(&["build"], &[]);
    let (summary, blockers) = summarize_with(
        vec![
            run("build", "completed", Some("success")),
            run("docs", "completed", Some("skipped")),
        ],
        vec![],
        false,
        &policy,
    );
    assert_eq!(summary.verdict, "PASS");
    assert_eq!(summary.skipped, 1);
    assert!(blockers.is_empty(), "unexpected blockers: {blockers:?}");
}

/// A skip on the required context itself still blocks, and the blocker names
/// the skip rather than a missing check.
#[test]
fn a_required_skip_blocks_and_is_named_as_a_skip() {
    let policy = known_policy(&["docs"], &[]);
    let (summary, blockers) = summarize_with(
        vec![run("docs", "completed", Some("skipped"))],
        vec![],
        false,
        &policy,
    );
    assert_eq!(summary.verdict, "FAIL");
    assert!(
        blockers.iter().any(|b| b.contains("was skipped")),
        "unexpected blockers: {blockers:?}"
    );
}

/// With no established required set, a skip is `UNKNOWN`, never a pass: the
/// tool cannot claim "nothing requires this" without having read the rules.
#[test]
fn an_undetermined_required_set_makes_a_skip_unknown() {
    let (summary, blockers) = summarize(
        vec![run("docs", "completed", Some("skipped"))],
        vec![],
        false,
    );
    assert_eq!(summary.verdict, "UNKNOWN");
    assert!(
        blockers
            .iter()
            .any(|b| b.contains("the required set could not be read")),
        "unexpected blockers: {blockers:?}"
    );
}

#[test]
fn a_completed_run_without_conclusion_blocks() {
    let (summary, blockers) = summarize(vec![run("test", "completed", None)], vec![], false);
    assert_eq!(summary.unknown.len(), 1);
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
    let policy = checks::ChecksPolicy {
        allow_no_ci: true,
        ..checks::ChecksPolicy::default()
    };
    let (summary, blockers) = summarize_with(vec![], vec![], true, &policy);
    assert!(!summary.incomplete);
    assert!(blockers.is_empty(), "unexpected blockers: {blockers:?}");
}

#[test]
fn bot_logins_cover_app_slugs_bot_suffixes_and_dependency_bots() {
    // Accepted: the `[bot]` suffix, the bare dependency bots, and the `app/`
    // form `gh pr list --json author` reports for an App author (#514).
    for login in [
        "renovate[bot]",
        "app/dependabot",
        "dependabot",
        "app/renovate",
        "dependabot[bot]",
        " app/dependabot ", // a padded payload is still the same login
    ] {
        assert!(is_bot_login(login), "{login} must classify as a bot");
    }
    // Rejected: the match is a whole login, never a substring, so a hostile
    // account cannot present itself as a bot by carrying its name.
    for login in [
        "dependabot-helper",
        "notdependabot",
        "app/dependabot-helper",
        "renovate-bot",
        "app/",
        "",
    ] {
        assert!(!is_bot_login(login), "{login} must not classify as a bot");
    }
}

#[test]
fn the_server_is_bot_flag_is_authoritative_over_the_name_forms() {
    // A login no name form matches is still a bot when `GitHub` says so: the
    // authoritative signal is the flag, not this harness's vocabulary.
    assert!(is_bot_author("some-app", true));
    assert!(is_bot_author("app/dependabot", true));
    // Absent the flag, the documented names remain the fallback.
    assert!(is_bot_author("app/dependabot", false));
    assert!(!is_bot_author("octocat", false));
}
