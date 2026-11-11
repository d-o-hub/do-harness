//! Cross-platform coverage of the merge-authorizing CI classifier.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;
use crate::pr::gh::{CheckApp, CheckRun, CommitStatus};

fn run(name: &str, status: &str, conclusion: Option<&str>) -> CheckRun {
    CheckRun {
        id: None,
        name: name.to_owned(),
        status: status.to_owned(),
        conclusion: conclusion.map(str::to_owned),
        html_url: None,
        app: Some(CheckApp {
            id: None,
            slug: Some("github-actions".to_owned()),
        }),
        started_at: None,
        completed_at: None,
    }
}

fn app_run(name: &str, app: &str, conclusion: &str) -> CheckRun {
    let mut check = run(name, "completed", Some(conclusion));
    check.app = Some(CheckApp {
        id: None,
        slug: Some(app.to_owned()),
    });
    check
}

fn status(context: &str, state: &str) -> CommitStatus {
    CommitStatus {
        context: context.to_owned(),
        state: state.to_owned(),
        target_url: None,
    }
}

fn classify_with(
    runs: Vec<CheckRun>,
    statuses: Vec<CommitStatus>,
    policy: &ChecksPolicy,
) -> ChecksReport {
    classify(7, "abc123", runs, statuses, policy, false)
}

#[test]
fn zero_checks_and_statuses_is_unknown_without_policy() {
    let report = classify_with(vec![], vec![], &ChecksPolicy::default());
    assert_eq!(report.verdict, Verdict::Unknown);
    assert!(
        report
            .blockers
            .iter()
            .any(|b| b.code == "no_checks_reported")
    );
}

#[test]
fn zero_checks_passes_only_with_an_explicit_no_ci_policy() {
    let policy = ChecksPolicy {
        allow_no_ci: true,
        ..ChecksPolicy::default()
    };
    let report = classify_with(vec![], vec![], &policy);
    assert_eq!(report.verdict, Verdict::Pass);
    assert!(report.blockers.is_empty());
}

#[test]
fn the_101st_failing_check_is_not_hidden_by_the_first_100() {
    let mut runs: Vec<CheckRun> = (0..100)
        .map(|i| run(&format!("check-{i}"), "completed", Some("success")))
        .collect();
    runs.push(run("check-101", "completed", Some("failure")));
    let report = classify_with(runs, vec![], &ChecksPolicy::default());
    assert_eq!(report.verdict, Verdict::Fail);
    assert!(
        report
            .checks
            .iter()
            .any(|c| c.name == "check-101" && c.verdict == Verdict::Fail)
    );
}

#[test]
fn a_missing_required_check_is_unknown_even_when_everything_else_is_green() {
    let policy = ChecksPolicy {
        required: vec!["verify".to_owned()],
        ..ChecksPolicy::default()
    };
    let report = classify_with(
        vec![run("docs", "completed", Some("success"))],
        vec![],
        &policy,
    );
    assert_eq!(report.verdict, Verdict::Unknown);
    assert!(
        report
            .blockers
            .iter()
            .any(|b| b.code == "missing_required_check")
    );
}

#[test]
fn a_required_check_that_is_only_pending_is_not_present() {
    let policy = ChecksPolicy {
        required: vec!["verify".to_owned()],
        ..ChecksPolicy::default()
    };
    let report = classify_with(vec![run("verify", "in_progress", None)], vec![], &policy);
    assert_eq!(report.verdict, Verdict::Pending);
    assert!(
        report
            .blockers
            .iter()
            .any(|b| b.code == "missing_required_check")
    );
}

#[test]
fn bot_and_external_app_failures_are_included() {
    let report = classify_with(
        vec![
            app_run("dependabot", "dependabot", "failure"),
            app_run("Codacy", "codacy", "failure"),
        ],
        vec![],
        &ChecksPolicy::default(),
    );
    assert_eq!(report.verdict, Verdict::Fail);
    assert_eq!(report.checks.len(), 2);
    assert!(report.checks.iter().all(|c| c.verdict == Verdict::Fail));
    assert!(report.checks.iter().any(|c| c.source == "dependabot"));
    assert!(report.checks.iter().any(|c| c.source == "codacy"));
}

#[test]
fn cancelled_and_action_required_never_pass() {
    for conclusion in [
        "cancelled",
        "action_required",
        "timed_out",
        "startup_failure",
    ] {
        let report = classify_with(
            vec![run("verify", "completed", Some(conclusion))],
            vec![],
            &ChecksPolicy::default(),
        );
        assert_eq!(report.verdict, Verdict::Fail, "{conclusion} must not pass");
    }
}

#[test]
fn skipped_and_neutral_pass_only_when_allowlisted() {
    let mut policy = ChecksPolicy::default();
    policy.allow_skips.insert("docs".to_owned());
    let allowed = classify_with(
        vec![run("docs", "completed", Some("skipped"))],
        vec![],
        &policy,
    );
    assert_eq!(allowed.verdict, Verdict::Pass);
    assert_eq!(allowed.checks[0].verdict, Verdict::Skip);

    let denied = classify_with(
        vec![run("lint", "completed", Some("neutral"))],
        vec![],
        &policy,
    );
    assert_eq!(denied.verdict, Verdict::Fail);
    assert_eq!(
        denied.checks[0].reason.as_deref(),
        Some("unallowlisted skip")
    );
}

#[test]
fn repeated_status_contexts_keep_the_newest_state() {
    let report = classify_with(
        vec![],
        vec![
            status("ci/legacy", "success"),
            status("ci/legacy", "failure"),
        ],
        &ChecksPolicy::default(),
    );
    assert_eq!(report.checks.len(), 1);
    assert_eq!(report.checks[0].state, "success");
    assert_eq!(report.verdict, Verdict::Pass);
}

#[test]
fn the_report_carries_head_identity_source_and_completion_state() {
    let report = classify_with(
        vec![app_run("verify", "github-actions", "success")],
        vec![status("legacy-ci", "success")],
        &ChecksPolicy::default(),
    );
    assert_eq!(report.head_sha, "abc123");
    assert_eq!(report.checks.len(), 2);
    let check = report.checks.iter().find(|c| c.kind == "check").unwrap();
    assert_eq!(check.name, "verify");
    assert_eq!(check.source, "github-actions");
    assert_eq!(check.state, "completed");
    assert_eq!(check.conclusion.as_deref(), Some("success"));
    let legacy = report.checks.iter().find(|c| c.kind == "status").unwrap();
    assert_eq!(legacy.source, "status");
    assert_eq!(legacy.state, "success");
}

#[test]
fn a_conflicting_pr_marks_its_check_set_incomplete() {
    let report = classify(
        7,
        "abc123",
        vec![run("verify", "completed", Some("success"))],
        vec![],
        &ChecksPolicy::default(),
        true,
    );
    assert_eq!(report.verdict, Verdict::Unknown);
    assert!(
        report
            .blockers
            .iter()
            .any(|b| b.code == "checks_incomplete")
    );
}

/// A conflicted head with no reported checks is still `UNKNOWN`, but the
/// blocker names the empty suite rather than incompleteness: there is nothing
/// to call incomplete. Downstream code keys on the code, so pin it.
#[test]
fn a_conflicting_pr_with_no_checks_reports_the_empty_suite() {
    let report = classify(7, "abc123", vec![], vec![], &ChecksPolicy::default(), true);
    assert_eq!(report.verdict, Verdict::Unknown);
    assert!(
        report
            .blockers
            .iter()
            .any(|b| b.code == "no_checks_reported")
    );
    assert!(
        !report
            .blockers
            .iter()
            .any(|b| b.code == "checks_incomplete")
    );
}

#[test]
fn a_malformed_policy_is_an_error_not_a_permissive_default() {
    let dir = tempfile::tempdir().unwrap();
    let policy_dir = dir.path().join(POLICY_DIR);
    std::fs::create_dir_all(&policy_dir).unwrap();
    std::fs::write(policy_dir.join(POLICY_FILE), "allow_no_ci = \"yes\"\n").unwrap();
    assert!(load_policy(dir.path()).is_err());
}

#[test]
fn the_legacy_allowlist_is_still_honoured() {
    let dir = tempfile::tempdir().unwrap();
    let policy_dir = dir.path().join(POLICY_DIR);
    std::fs::create_dir_all(&policy_dir).unwrap();
    std::fs::write(policy_dir.join(ALLOW_FILE), "# comment\ndocs\n").unwrap();
    let policy = load_policy(dir.path()).unwrap();
    assert!(policy.allow_skips.contains("docs"));
}
