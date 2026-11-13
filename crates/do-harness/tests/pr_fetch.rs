//! The prefetch *feature* (#520): does a clone without the commits resolve?
//!
//! The issue's claim is narrow and easy to fake, so every test here runs against
//! a **real bare remote** and a **real shallow clone**, and the fake `gh` only
//! supplies object ids that remote actually created. A stub `git` would prove
//! nothing about ancestry.
//!
//! The bounds — idempotence, the `--no-fetch` escape hatch, failure handling, and
//! the guarantee that no branch or tag is written — are in `pr_fetch_safety.rs`.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

#[cfg(unix)]
use support::pr_fetch::{setup, setup_with_base_absent};
#[cfg(unix)]
use support::pr_fetch_run::{
    assert_refs_untouched, connected, refs, rev_exists, run_no_effect, snapshot,
};

/// The case that actually breaks: the base is present, the head is not, and the
/// shallow graft severs their ancestry.
///
/// Asserted first, because a fixture that merely lacks both objects is a much
/// easier problem — fetching the base brings its ancestry with it and the pair
/// connects by accident. This shape is the one where fetching is *not* enough
/// and the history has to be deepened.
#[cfg(unix)]
#[test]
fn a_shallow_graft_is_repaired_so_the_verdict_resolves() {
    let fixture = setup(true);
    let before = snapshot(&fixture.clone);
    let refs_before = refs(&fixture.clone);

    // The premise, asserted rather than assumed.
    assert!(
        rev_exists(&fixture.clone, &fixture.base),
        "the clone is taken at the base tip, so the base must be present"
    );
    assert!(
        !rev_exists(&fixture.clone, &fixture.head),
        "the fixture must start without the pull request head"
    );
    assert!(
        support::git_command(&fixture.clone)
            .args(["rev-parse", "--is-shallow-repository"])
            .output()
            .expect("spawn git")
            .stdout
            .starts_with(b"true"),
        "the clone must be shallow, or the graft is not exercised"
    );
    // Fetching the head alone leaves the pair disconnected: this is the defect,
    // reproduced here rather than described.
    support::git(
        &fixture.clone,
        &[
            "fetch",
            "-q",
            "--no-tags",
            "origin",
            "+refs/pull/1263/head:refs/remotes/pr-triage/1263",
        ],
    );
    assert!(
        rev_exists(&fixture.clone, &fixture.head),
        "the head arrived"
    );
    assert!(
        !connected(&fixture.clone, &fixture.base, &fixture.head),
        "with the head fetched and no deepening, the graft must still sever ancestry"
    );
    // Reset so the command under test performs its own prefetch.
    support::git(
        &fixture.clone,
        &["update-ref", "-d", "refs/remotes/pr-triage/1263"],
    );

    let (code, report, stderr) = run_no_effect(&fixture, &[]);
    assert_eq!(code, Some(0), "an analysis was produced: {stderr}");
    assert_eq!(
        report["merge_impact"]["verdict"], "TREE_EFFECT",
        "the PR changes a file, so merging changes the base tree: {report}"
    );
    assert_eq!(
        report["merge_impact"]["method"], "merge-tree",
        "the verdict must come from the local proof, not the compare API: {report}"
    );
    assert_eq!(
        report["merge_impact"]["base_verified"], true,
        "the base must be the reported remote commit: {report}"
    );
    assert_eq!(
        report["merge_impact"]["base_sha"], fixture.base,
        "the proof must be against GitHub's baseRefOid: {report}"
    );
    assert_eq!(
        snapshot(&fixture.clone),
        before,
        "the worktree, branch, and HEAD must be untouched"
    );
    assert_refs_untouched(&fixture.clone, &refs_before);
}

/// The shape the issue names: the reported base is absent *and* disconnected.
///
/// Weaker than the graft case, but it is what production reported, and it
/// exercises the base-by-object-id fetch rather than only the deepening.
#[cfg(unix)]
#[test]
fn a_shallow_clone_resolves_the_verdict_without_a_manual_fetch() {
    let fixture = setup_with_base_absent(true);
    let before = snapshot(&fixture.clone);
    let refs_before = refs(&fixture.clone);

    assert!(
        !rev_exists(&fixture.clone, &fixture.base),
        "the fixture must start without the reported base"
    );
    assert!(
        !rev_exists(&fixture.clone, &fixture.head),
        "the fixture must start without the pull request head"
    );

    let (code, report, stderr) = run_no_effect(&fixture, &[]);
    assert_eq!(code, Some(0), "an analysis was produced: {stderr}");

    assert_eq!(
        report["merge_impact"]["verdict"], "TREE_EFFECT",
        "the PR changes a file, so merging changes the base tree: {report}"
    );
    assert_eq!(
        report["merge_impact"]["method"], "merge-tree",
        "the verdict must come from the local proof, not the compare API: {report}"
    );
    assert_eq!(
        report["merge_impact"]["base_verified"], true,
        "the base must be the reported remote commit: {report}"
    );
    assert_eq!(
        report["merge_impact"]["base_sha"], fixture.base,
        "the proof must be against GitHub's baseRefOid: {report}"
    );

    // Immutability: the fetch moved objects, nothing else.
    assert_eq!(
        snapshot(&fixture.clone),
        before,
        "the worktree, branch, and HEAD must be untouched"
    );
    assert_refs_untouched(&fixture.clone, &refs_before);
}

/// A no-effect PR still proves `NO_TREE_EFFECT` and recommends closure, which is
/// the decision the fetch exists to make available.
#[cfg(unix)]
#[test]
fn a_shallow_clone_proves_no_tree_effect_and_recommends_closure() {
    let fixture = setup(false);
    let before = snapshot(&fixture.clone);
    let refs_before = refs(&fixture.clone);

    let (code, report, stderr) = run_no_effect(&fixture, &[]);
    assert_eq!(code, Some(0), "{stderr}");
    assert_eq!(
        report["merge_impact"]["verdict"], "NO_TREE_EFFECT",
        "an empty commit cannot change the base tree: {report}"
    );
    assert_eq!(
        report["closure_recommendation"], true,
        "a proven no-tree-effect merge on a verified base is closure-eligible: {report}"
    );
    assert_eq!(snapshot(&fixture.clone), before, "no worktree change");
    assert_refs_untouched(&fixture.clone, &refs_before);
}
