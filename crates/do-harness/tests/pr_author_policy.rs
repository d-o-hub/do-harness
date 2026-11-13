//! Author-policy acceptance fixtures (#519).
//!
//! Each case drives the real binary through `pr sweep --execute --merge`
//! against a fake `gh`, so the governance boundary is exercised end to end: who
//! the sweep may merge and what "green" means for an external author. The
//! companion file `pr_author_policy_write.rs` proves the other half — that
//! widening *merging* never widens *writing* — and covers the configuration and
//! usage boundaries.
//!
//! The external fixtures all answer `404` from the collaborator-permission
//! endpoint, which is what makes a human login `Owner::External` rather than
//! owned (#514).

#![allow(clippy::unwrap_used, clippy::expect_used)]

#[cfg(unix)]
use serde_json::{Value, json};

mod support;

#[cfg(unix)]
use support::pr_sweep::{HEAD, assert_read_only, external_green_pr, mutations, run_sweep, setup};

/// The default policy, spelled explicitly so the fixture states what it assumes.
#[cfg(unix)]
const OWNED: &str = "owned";
/// The relaxed policy.
#[cfg(unix)]
const RELAXED: &str = "owned-or-external-green";

/// The default policy still refuses an external author, and says so once.
#[cfg(unix)]
#[test]
fn an_external_green_pr_is_skipped_under_the_default_policy() {
    let (dir, bin) = setup();
    let root = dir.path();
    external_green_pr(root, 2);

    let (code, stdout, stderr) = run_sweep(
        root,
        &bin,
        &[
            "pr",
            "sweep",
            "--pr",
            "2",
            "--execute",
            "--merge",
            "--format",
            "json",
        ],
    );
    assert_eq!(code, Some(0), "{stderr}");
    let report: Value = serde_json::from_str(&stdout).expect("JSON report");
    assert_eq!(report["prs"][0]["outcome"], "skipped", "{stdout}");
    assert_eq!(report["prs"][0]["reasons"][0]["code"], "external");
    assert_eq!(report["prs"][0]["authorized"], false);
    assert_eq!(
        report["author_policy"], OWNED,
        "the compiled-in default is `owned`: {stdout}"
    );
    assert_eq!(mutations(root), [] as [String; 0], "{stdout}");
}

/// The relaxed policy merges the same PR, and the report records which policy
/// authorised it.
#[cfg(unix)]
#[test]
fn an_external_green_pr_merges_under_the_relaxed_policy() {
    let (dir, bin) = setup();
    let root = dir.path();
    external_green_pr(root, 3);

    let (code, stdout, stderr) = run_sweep(
        root,
        &bin,
        &[
            "pr",
            "sweep",
            "--pr",
            "3",
            "--execute",
            "--merge",
            "--author-policy",
            RELAXED,
            "--format",
            "json",
        ],
    );
    assert_eq!(
        code,
        Some(0),
        "an external green PR may merge: {stderr}\n{stdout}"
    );
    let report: Value = serde_json::from_str(&stdout).expect("JSON report");
    assert_eq!(report["prs"][0]["outcome"], "merged", "{stdout}");
    assert_eq!(report["prs"][0]["authorized"], true);
    assert_eq!(report["author_policy"], RELAXED, "{stdout}");
    assert_eq!(report["summary"]["merged"], 1);
    assert_eq!(report["summary"]["skipped"], 0);

    let cmds = mutations(root);
    assert_eq!(
        cmds,
        vec![format!("pr merge 3 --squash --match-head-commit {HEAD}")],
        "exactly one mutation, and it is the merge itself"
    );
    assert_read_only(root);
}

/// A failing required check keeps the PR unmerged under *both* policies: the
/// relaxation covers authorship, never CI.
#[cfg(unix)]
#[test]
fn a_failing_required_check_blocks_under_either_policy() {
    for policy in [OWNED, RELAXED] {
        let (dir, bin) = setup();
        let root = dir.path();
        external_green_pr(root, 4);
        std::fs::write(
            root.join("check_runs.json"),
            json!({
                "total_count": 1,
                "check_runs": [{
                    "id": 1,
                    "name": "pr-required",
                    "status": "completed",
                    "conclusion": "failure",
                    "app": { "id": 15368, "slug": "github-actions" }
                }]
            })
            .to_string(),
        )
        .unwrap();

        let (code, stdout, stderr) = run_sweep(
            root,
            &bin,
            &[
                "pr",
                "sweep",
                "--pr",
                "4",
                "--execute",
                "--merge",
                "--author-policy",
                policy,
                "--format",
                "json",
            ],
        );
        assert_eq!(code, Some(0), "[{policy}] {stderr}\n{stdout}");
        let report: Value = serde_json::from_str(&stdout).expect("JSON report");
        assert_ne!(
            report["prs"][0]["outcome"], "merged",
            "[{policy}] a red required check must never merge: {stdout}"
        );
        assert!(
            mutations(root).iter().all(|cmd| !cmd.contains("pr merge")),
            "[{policy}] {stdout}"
        );
    }
}

/// An unresolved review thread keeps the PR unmerged, and the tool never
/// resolves it on someone else's branch.
#[cfg(unix)]
#[test]
fn an_unresolved_thread_blocks_and_is_never_resolved() {
    let (dir, bin) = setup();
    let root = dir.path();
    external_green_pr(root, 5);
    std::fs::write(
        root.join("graphql.json"),
        json!({
            "data": { "repository": { "pullRequest": { "reviewThreads": {
                "nodes": [{
                    "id": "thread-1",
                    "isResolved": false,
                    "path": "src/lib.rs",
                    "line": 1,
                    "comments": { "nodes": [
                        { "author": { "login": "reviewer" }, "body": "Fix this." }
                    ] }
                }]
            } } } }
        })
        .to_string(),
    )
    .unwrap();

    let (code, stdout, stderr) = run_sweep(
        root,
        &bin,
        &[
            "pr",
            "sweep",
            "--pr",
            "5",
            "--execute",
            "--merge",
            "--author-policy",
            RELAXED,
            "--format",
            "json",
        ],
    );
    assert_eq!(code, Some(0), "{stderr}");
    let report: Value = serde_json::from_str(&stdout).expect("JSON report");
    assert_ne!(
        report["prs"][0]["outcome"], "merged",
        "an unresolved thread must never merge: {stdout}"
    );
    assert_eq!(
        report["prs"][0]["reasons"][0]["code"], "conversation_unresolved",
        "the blocker is the thread itself: {stdout}"
    );
    // The findings are reported rather than acted on.
    assert!(
        mutations(root).is_empty(),
        "no thread resolution, no merge: {:#?}",
        mutations(root)
    );
    assert_read_only(root);
}
