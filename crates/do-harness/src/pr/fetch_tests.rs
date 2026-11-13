//! Unit table for the merge-impact prefetch (#520).
//!
//! The network is not involved: `plan`, `head_present` and `head_available` are
//! questions about which objects a local repository already has, and the
//! failure formatting is a pure function. The end-to-end fetch behaviour — a
//! real shallow clone becoming provable, the worktree staying untouched, an
//! unreachable remote yielding `UNKNOWN` — lives in `tests/pr_fetch.rs`,
//! because it needs a real remote.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::Path;
use std::process::Command;
use std::time::Duration;

use super::{
    DEEPEN_INCREMENT, FETCH_TIMEOUT, MAX_DEPTHEN_STEPS, PrefetchOutcome, REF_PREFIX, Step,
    head_available, head_present, parse_timeout, plan,
};

/// Builds a repository with one commit and returns its SHA.
fn repo_with_commit(dir: &Path) -> String {
    let git = |args: &[&str]| {
        let output = Command::new("git")
            .current_dir(dir)
            .args(args)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_AUTHOR_NAME", "t")
            .env("GIT_AUTHOR_EMAIL", "t@t")
            .env("GIT_COMMITTER_NAME", "t")
            .env("GIT_COMMITTER_EMAIL", "t@t")
            .output()
            .expect("spawn git");
        assert!(
            output.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        output
    };
    git(&["init", "-q", "-b", "main"]);
    std::fs::write(dir.join("a.txt"), "one\n").unwrap();
    git(&["add", "a.txt"]);
    git(&["commit", "-q", "-m", "base"]);
    String::from_utf8_lossy(&git(&["rev-parse", "HEAD"]).stdout)
        .trim()
        .to_owned()
}

/// Creates the tracked pull-request ref for `number` at `sha`.
fn track_head(dir: &Path, number: u64, sha: &str) {
    let status = Command::new("git")
        .current_dir(dir)
        .args(["update-ref", &format!("{REF_PREFIX}/{number}"), sha])
        .status()
        .expect("spawn git");
    assert!(status.success());
}

/// A full clone fetches nothing, even though it has never heard of our ref
/// namespace.
///
/// This is the case that makes the prefetch free on the common path. The head is
/// judged by the *commit*, not by the ref: a normal clone that checked the
/// branch out has the commit and no `refs/remotes/pr-triage/*`, and fetching
/// anyway would cost network on every single `pr no-effect` run.
#[test]
fn a_full_clone_with_both_commits_plans_no_fetch() {
    let dir = tempfile::tempdir().unwrap();
    let sha = repo_with_commit(dir.path());

    assert!(
        !head_present(dir.path(), 7),
        "the fixture must not pre-create the tracked ref"
    );
    assert!(
        head_available(dir.path(), 7, &sha),
        "the head commit itself is present"
    );
    assert_eq!(
        plan(dir.path(), 7, &sha, &sha),
        Vec::<Step>::new(),
        "nothing is missing, so nothing is fetched"
    );
}

/// Both sides missing plans exactly the two documented fetches, in order.
#[test]
fn a_missing_base_and_head_plan_the_two_documented_fetches() {
    let dir = tempfile::tempdir().unwrap();
    let sha = repo_with_commit(dir.path());
    let missing = "9".repeat(40);

    let steps = plan(dir.path(), 42, &missing, &missing);
    assert_eq!(steps.len(), 2, "{steps:#?}");

    // The base is fetched first: it is the smaller, more useful object.
    assert_eq!(
        steps[0].args,
        vec!["--no-tags".to_owned(), "origin".to_owned(), missing.clone()]
    );
    assert_eq!(
        steps[0].command(),
        format!("git fetch --no-tags origin {missing}")
    );

    // The head goes only into the pr-triage namespace, never a branch.
    assert_eq!(
        steps[1].args,
        vec![
            "--no-tags".to_owned(),
            "origin".to_owned(),
            format!("+refs/pull/42/head:{REF_PREFIX}/42")
        ]
    );
    assert!(
        steps[1].args[2].ends_with(&format!(":{REF_PREFIX}/42")),
        "the head must land under {}, never a branch: {}",
        REF_PREFIX,
        steps[1].args[2]
    );

    // The present commit is not re-fetched.
    assert!(
        plan(dir.path(), 42, &sha, &sha).is_empty(),
        "an already-present pair must not be fetched again"
    );
}

/// Each missing side is planned independently.
#[test]
fn only_the_missing_side_is_planned() {
    let dir = tempfile::tempdir().unwrap();
    let sha = repo_with_commit(dir.path());

    // Base present, head missing.
    let steps = plan(dir.path(), 3, &sha, &"6".repeat(40));
    assert_eq!(steps.len(), 1, "{steps:#?}");
    assert!(steps[0].args[2].contains("refs/pull/3/head"), "{steps:#?}");

    // Head present, base missing.
    let missing = "8".repeat(40);
    let steps = plan(dir.path(), 3, &missing, &sha);
    assert_eq!(steps.len(), 1, "{steps:#?}");
    assert_eq!(steps[0].args[2], missing, "{steps:#?}");
}

/// A tracked ref alone satisfies the head side: a shallow fetch of the pull ref
/// can carry the ref while the commit stays absent, and re-fetching it would
/// never converge.
#[test]
fn a_tracked_head_ref_counts_as_available() {
    let dir = tempfile::tempdir().unwrap();
    let sha = repo_with_commit(dir.path());
    track_head(dir.path(), 11, &sha);

    assert!(head_present(dir.path(), 11));
    assert!(
        head_available(dir.path(), 11, &"5".repeat(40)),
        "the ref resolves even though the reported commit does not"
    );
    // Only the base is left to fetch.
    let steps = plan(dir.path(), 11, &"5".repeat(40), &"5".repeat(40));
    assert_eq!(steps.len(), 1, "{steps:#?}");
    assert_eq!(steps[0].args[2], "5".repeat(40), "{steps:#?}");
}

/// An empty base OID plans no base fetch: an absent OID is not a commit to ask
/// for, and `git fetch origin ""` would be a confusing failure.
#[test]
fn an_empty_base_oid_is_not_fetched() {
    let dir = tempfile::tempdir().unwrap();
    repo_with_commit(dir.path());
    let steps = plan(dir.path(), 5, "", &"4".repeat(40));
    assert_eq!(steps.len(), 1, "{steps:#?}");
    assert!(
        steps[0].args[2].contains("refs/pull/5/head"),
        "only the head is fetched: {steps:#?}"
    );
}

/// A missing repository is not a panic: the plan simply asks for both.
#[test]
fn a_non_repository_plans_both_fetches() {
    let dir = tempfile::tempdir().unwrap();
    let steps = plan(dir.path(), 9, &"7".repeat(40), &"7".repeat(40));
    assert_eq!(steps.len(), 2, "{steps:#?}");
}

/// The outcome vocabulary is what the caller switches on, and each variant must
/// stay distinct: collapsing two of them would let a failed fetch look like a
/// successful one.
#[test]
fn the_four_outcomes_stay_distinct() {
    let unavailable = PrefetchOutcome::Unavailable {
        command: "git fetch origin x".to_owned(),
        reason: "network".to_owned(),
    };
    // A failure is the only outcome that carries a next action.
    assert!(unavailable.warning().is_some());
    // "Asked not to fetch" must never be indistinguishable from "fetched".
    assert_ne!(PrefetchOutcome::Disabled, PrefetchOutcome::AlreadyPresent);
    assert_ne!(PrefetchOutcome::Fetched, PrefetchOutcome::AlreadyPresent);
    assert_ne!(PrefetchOutcome::Disabled, PrefetchOutcome::Fetched);
}

/// A failure carries the command to run, so the warning is a next action rather
/// than a restatement of the symptom.
#[test]
fn an_unavailable_outcome_warns_with_the_command() {
    let outcome = PrefetchOutcome::Unavailable {
        command: "git fetch --no-tags origin deadbeef".to_owned(),
        reason: "base commit deadbeef: git fetch exited 128: could not read from remote".to_owned(),
    };
    let warning = outcome.warning().expect("a failure must warn");
    assert!(
        warning.contains("git fetch --no-tags origin deadbeef"),
        "{warning}"
    );
    assert!(warning.contains("could not read from remote"), "{warning}");
    assert!(warning.contains("retry"), "{warning}");

    // The usable outcomes stay silent: a successful prefetch is not news, and a
    // deliberately disabled one is the operator's own choice.
    assert!(PrefetchOutcome::AlreadyPresent.warning().is_none());
    assert!(PrefetchOutcome::Fetched.warning().is_none());
    assert!(PrefetchOutcome::Disabled.warning().is_none());
}

/// The bound is finite, so a stalled transport cannot hang a proof.
#[test]
fn the_fetch_bound_is_finite_and_short() {
    assert!(FETCH_TIMEOUT.as_secs() > 0);
    assert!(
        FETCH_TIMEOUT.as_secs() <= 120,
        "a proof step must not wait minutes: {FETCH_TIMEOUT:?}"
    );
}

/// The namespace is a remote-tracking one, which can never be the checked-out
/// ref — that is what makes writing it safe inside an authorization path.
#[test]
fn the_ref_namespace_cannot_be_a_checked_out_branch() {
    assert!(REF_PREFIX.starts_with("refs/remotes/"), "{REF_PREFIX}");
    assert!(!REF_PREFIX.starts_with("refs/heads/"), "{REF_PREFIX}");
    assert!(!REF_PREFIX.starts_with("refs/tags/"), "{REF_PREFIX}");
}

/// The environment override is a real bound, not a bypass.
///
/// The property that matters is that a bad value falls back to the documented
/// default instead of removing the bound: "no timeout" is the failure mode that
/// turns a stalled transport into a hung merge decision.
#[test]
fn the_timeout_override_never_removes_the_bound() {
    let default = FETCH_TIMEOUT;
    for bad in [
        None,
        Some(""),
        Some("0"),
        Some("-5"),
        Some("abc"),
        Some("1.5"),
        Some("  "),
    ] {
        assert_eq!(
            parse_timeout(bad),
            default,
            "{bad:?} must fall back to the bound, not disable it"
        );
    }
    // A valid value is honoured, including with surrounding whitespace.
    assert_eq!(parse_timeout(Some("7")), Duration::from_secs(7));
    assert_eq!(parse_timeout(Some(" 7\n")), Duration::from_secs(7));
    // The value is seconds, so a large one is still finite.
    assert_eq!(parse_timeout(Some("3600")), Duration::from_secs(3600));
}

/// Deepening is bounded in both directions.
///
/// The gap a pull request can open is unbounded, so the repair must be finite:
/// an unrelated history never connects, and an unbounded loop would download a
/// repository without ever answering the question. The *coverage* is what keeps
/// it useful — the attempts compound, so a fork point tens of commits back is
/// crossed in one round trip rather than one commit at a time.
#[test]
fn deepening_is_finite_but_covers_a_useful_gap() {
    const { assert!(MAX_DEPTHEN_STEPS > 0, "the repair must be attempted at all") };
    const { assert!(DEEPEN_INCREMENT > 0) };
    // Attempts deepen by growing multiples of the increment, so the total walk
    // is the triangular number, not the increment alone.
    let covered: u64 = (1..=u64::from(MAX_DEPTHEN_STEPS))
        .map(|step| u64::from(DEEPEN_INCREMENT) * step)
        .sum();
    assert!(
        covered >= 1000,
        "the repair should cross a realistic fork distance; it covers {covered} commits"
    );
    // ...and stays finite enough that a hostile repository cannot pin the run.
    const { assert!(MAX_DEPTHEN_STEPS <= 16, "attempts are round trips") };
}
