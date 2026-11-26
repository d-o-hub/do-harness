//! Re-triggered workflows leave more than one check run per name on one head.
//!
//! `GitHub` gates the head on the newest run, so these cover the classifier's
//! matching rule: keep every run in the report, mark the ones a newer run
//! replaced, and never let a superseded verdict decide the head.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;

fn run_with_id(id: u64, name: &str, conclusion: &str) -> CheckRun {
    let mut check = app_run(name, "github-actions", conclusion);
    check.id = Some(id);
    check
}

#[test]
fn a_superseded_run_is_reported_but_does_not_decide_the_head() {
    let report = classify_with(
        vec![
            run_with_id(9, "issue-link", "success"),
            run_with_id(4, "issue-link", "failure"),
        ],
        vec![],
        &ChecksPolicy::default(),
    );
    let checks = &report.checks;
    assert_eq!(checks.len(), 2, "both runs stay visible in the report");
    let stale = checks.iter().find(|c| c.superseded).expect("stale run");
    assert_eq!(stale.conclusion.as_deref(), Some("failure"));
    assert_eq!(report.verdict, Verdict::Pass);
}

#[test]
fn the_greatest_run_id_is_current_whatever_the_page_order() {
    // The failing run is listed first, so only the id can identify the newer
    // run: this is the shape a re-triggered workflow produces.
    let report = classify_with(
        vec![
            run_with_id(4, "issue-link", "failure"),
            run_with_id(9, "issue-link", "success"),
        ],
        vec![],
        &ChecksPolicy::default(),
    );
    let current = report
        .checks
        .iter()
        .find(|c| !c.superseded)
        .expect("current");
    assert_eq!(current.conclusion.as_deref(), Some("success"));
    assert_eq!(report.verdict, Verdict::Pass);
}

#[test]
fn a_newer_check_run_failure_supersedes_an_older_success() {
    let report = classify_with(
        vec![
            run_with_id(9, "verify", "failure"),
            run_with_id(4, "verify", "success"),
        ],
        vec![],
        &ChecksPolicy::default(),
    );
    assert_eq!(report.checks.len(), 2);
    assert_eq!(report.verdict, Verdict::Fail);
}

#[test]
fn without_run_ids_the_first_reported_run_stays_current() {
    let report = classify_with(
        vec![
            app_run("issue-link", "github-actions", "success"),
            app_run("issue-link", "github-actions", "failure"),
        ],
        vec![],
        &ChecksPolicy::default(),
    );
    let current = report
        .checks
        .iter()
        .find(|c| !c.superseded)
        .expect("current");
    assert_eq!(current.conclusion.as_deref(), Some("success"));
    assert_eq!(report.verdict, Verdict::Pass);
}

#[test]
fn identical_skipped_runs_stay_reported_when_one_supersedes_the_rest() {
    // An adopter payload can carry several runs of one name (issue #1263
    // reported four). They all stay in the report, and the newest decides.
    let ids = [4, 5, 6, 7];
    let runs: Vec<CheckRun> = ids
        .iter()
        .map(|id| run_with_id(*id, "Maintain canonical drift issue", "skipped"))
        .collect();
    let report = classify_with(runs, vec![], &ChecksPolicy::default());
    assert_eq!(
        report.checks.len(),
        ids.len(),
        "every run stays in the report"
    );
    assert_eq!(
        report.checks.iter().filter(|c| c.superseded).count(),
        ids.len() - 1,
        "only the newest run of the name is current"
    );
    assert_ne!(report.verdict, Verdict::Fail, "a skip is never a failure");
}

#[test]
fn one_check_name_from_two_apps_is_not_collapsed() {
    let mut first = app_run("analyze", "github-actions", "success");
    first.id = Some(1);
    first.app = Some(CheckApp {
        id: Some(1),
        slug: Some("github-actions".to_owned()),
    });
    let mut second = app_run("analyze", "code-scanning", "failure");
    second.id = Some(2);
    second.app = Some(CheckApp {
        id: Some(2),
        slug: Some("code-scanning".to_owned()),
    });
    let report = classify_with(vec![first, second], vec![], &ChecksPolicy::default());
    assert_eq!(report.checks.len(), 2);
    assert!(
        report.checks.iter().all(|c| !c.superseded),
        "distinct apps are distinct required contexts"
    );
    assert_eq!(report.verdict, Verdict::Fail);
}
