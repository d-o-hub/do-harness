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

/// A policy whose required set is a decision the operator made.
fn known_policy(required: &[&str], allow_skips: &[&str]) -> ChecksPolicy {
    let mut policy = ChecksPolicy {
        required: required.iter().map(|s| (*s).to_owned()).collect(),
        required_known: true,
        ..ChecksPolicy::default()
    };
    policy
        .allow_skips
        .extend(allow_skips.iter().map(|s| (*s).to_owned()));
    policy
}

#[test]
fn an_optional_skip_passes_and_is_still_reported() {
    // The #516 shape: the required contexts are green and unrelated
    // maintenance jobs concluded `skipped`. Those are not failures.
    let policy = known_policy(&["CI / Required"], &[]);
    let report = classify_with(
        vec![
            run("CI / Required", "completed", Some("success")),
            run("Store Benchmark Results", "completed", Some("skipped")),
            run(
                "Maintain canonical drift issue",
                "completed",
                Some("skipped"),
            ),
        ],
        vec![],
        &policy,
    );
    assert_eq!(report.verdict, Verdict::Pass, "{:?}", report.blockers);
    assert!(report.blockers.is_empty());
    // Reported, not dropped: a skip that vanished from the report would hide
    // which jobs did not run.
    let benchmark = report
        .checks
        .iter()
        .find(|c| c.name == "Store Benchmark Results")
        .expect("the optional skip is listed");
    assert_eq!(benchmark.verdict, Verdict::Skip);
    assert_eq!(benchmark.reason.as_deref(), Some("optional skip"));
    assert_eq!(benchmark.source, "github-actions");
    assert_eq!(benchmark.conclusion.as_deref(), Some("skipped"));
}

#[test]
fn a_required_context_that_is_skipped_blocks_with_its_own_code() {
    let policy = known_policy(&["CI / Required"], &[]);
    let report = classify_with(
        vec![run("CI / Required", "completed", Some("skipped"))],
        vec![],
        &policy,
    );
    assert_eq!(report.verdict, Verdict::Fail);
    assert_eq!(report.checks[0].verdict, Verdict::Fail);
    assert_eq!(report.checks[0].reason.as_deref(), Some("skipped_required"));
    assert_eq!(report.blockers[0].code, "required_check_skipped");
}

#[test]
fn a_required_context_that_fails_blocks_as_a_failure() {
    let policy = known_policy(&["CI / Required"], &[]);
    let report = classify_with(
        vec![run("CI / Required", "completed", Some("failure"))],
        vec![],
        &policy,
    );
    assert_eq!(report.verdict, Verdict::Fail);
    assert_eq!(report.checks[0].reason.as_deref(), Some("failure"));
}

#[test]
fn a_required_context_absent_from_the_payload_is_missing() {
    let policy = known_policy(&["CI / Required"], &[]);
    let report = classify_with(
        vec![run("unrelated", "completed", Some("success"))],
        vec![],
        &policy,
    );
    assert_eq!(report.verdict, Verdict::Unknown);
    assert_eq!(report.blockers[0].code, "missing_required_check");
}

#[test]
fn an_allowlisted_skip_on_a_required_context_is_reported_as_a_contradiction() {
    // The two lists disagree, and the required entry wins. Saying so is the
    // point: a reviewer must not believe a gate was relaxed when it was not.
    let policy = known_policy(&["CI / Required"], &["CI / Required"]);
    let report = classify_with(
        vec![run("CI / Required", "completed", Some("success"))],
        vec![],
        &policy,
    );
    assert_eq!(report.verdict, Verdict::Pass);
    assert!(
        report
            .blockers
            .iter()
            .any(|b| b.code == "contradictory_required_skip"),
        "expected the contradiction to be reported: {:?}",
        report.blockers
    );
}

#[test]
fn an_unknown_required_set_never_lets_a_skip_pass() {
    // `required_known = false` is the state after the ruleset could not be
    // read. Treating "nothing requires this" as a fact would widen what
    // merges exactly when the tool failed.
    let policy = ChecksPolicy::default();
    assert!(!policy.required_known);
    let report = classify_with(
        vec![run("lint", "completed", Some("neutral"))],
        vec![],
        &policy,
    );
    assert_eq!(report.verdict, Verdict::Unknown);
    assert_eq!(report.checks[0].verdict, Verdict::Unknown);
}

#[test]
fn an_allowlisted_skip_still_passes_when_required_is_known() {
    let policy = known_policy(&[], &["docs"]);
    let allowed = classify_with(
        vec![run("docs", "completed", Some("skipped"))],
        vec![],
        &policy,
    );
    assert_eq!(allowed.verdict, Verdict::Pass);
    assert_eq!(allowed.checks[0].verdict, Verdict::Skip);
    assert_eq!(
        allowed.checks[0].reason.as_deref(),
        Some("allowlisted skip")
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

/// An `allow_skips`-only policy leaves the required set undetermined.
///
/// The distinction matters more than it looks: declaring `required = []` is an
/// operator saying "nothing gates a merge", while omitting the key says "I did
/// not answer that question". Collapsing the two would let a policy written to
/// widen one allowlist silently delete every server-required context, which is
/// the one direction a tightening knob must never move.
#[test]
fn a_policy_without_a_required_key_does_not_claim_a_required_set() {
    let dir = tempfile::tempdir().unwrap();
    let policy_dir = dir.path().join(POLICY_DIR);
    std::fs::create_dir_all(&policy_dir).unwrap();
    std::fs::write(policy_dir.join(POLICY_FILE), "allow_skips = [\"docs\"]\n").unwrap();
    let policy = load_policy(dir.path()).unwrap();
    assert!(
        !policy.required_known,
        "an absent `required` key must stay undetermined"
    );
    assert!(policy.allow_skips.contains("docs"));
}

/// The same file with an explicit empty list *is* a decision.
#[test]
fn an_explicitly_empty_required_list_is_a_decision() {
    let dir = tempfile::tempdir().unwrap();
    let policy_dir = dir.path().join(POLICY_DIR);
    std::fs::create_dir_all(&policy_dir).unwrap();
    std::fs::write(policy_dir.join(POLICY_FILE), "required = []\n").unwrap();
    let policy = load_policy(dir.path()).unwrap();
    assert!(policy.required_known);
    assert!(policy.required.is_empty(), "{:?}", policy.required);
}

/// The server's required set is used only when the local policy did not answer.
#[test]
fn the_resolved_required_set_fills_only_an_undetermined_policy() {
    use crate::pr::policy_source::{PolicySource, ResolvedPolicy};

    let resolved = ResolvedPolicy {
        source: PolicySource::Derived,
        known: true,
        ok: true,
        revision: None,
        rules: None,
        required: Some(vec!["CI / Required".to_owned()]),
        warnings: Vec::new(),
    };

    let filled =
        crate::pr::checks_policy::with_resolved_required(ChecksPolicy::default(), &resolved);
    assert!(filled.required_known);
    assert_eq!(filled.required, vec!["CI / Required".to_owned()]);

    // A local declaration wins: it is the auditable tightening knob.
    let local = known_policy(&["mine"], &[]);
    let kept = crate::pr::checks_policy::with_resolved_required(local, &resolved);
    assert_eq!(kept.required, vec!["mine".to_owned()]);
}

/// An unreadable source leaves the policy undetermined, never empty-and-known.
#[test]
fn an_unreadable_source_leaves_the_required_set_unknown() {
    use crate::pr::policy_source::{PolicySource, ResolvedPolicy};

    let resolved = ResolvedPolicy {
        source: PolicySource::Derived,
        known: false,
        ok: false,
        revision: None,
        rules: None,
        required: None,
        warnings: Vec::new(),
    };
    let policy =
        crate::pr::checks_policy::with_resolved_required(ChecksPolicy::default(), &resolved);
    assert!(!policy.required_known);
    // And a skip therefore cannot pass.
    let report = classify_with(
        vec![run("lint", "completed", Some("skipped"))],
        vec![],
        &policy,
    );
    assert_eq!(report.verdict, Verdict::Unknown);
}
