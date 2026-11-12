//! `BEHIND` branch recovery acceptance fixtures (#518).
//!
//! Each case drives the real binary through `pr sweep --execute --merge`
//! against a fake `gh`, so the ordered chain — API update, then the documented
//! dependency-bot rebase command, then a bounded wait and a full revalidation —
//! is exercised end to end rather than unit-tested around.
//!
//! The invariant under test throughout: this tool never merges on a receipt that
//! describes a head which has since moved, never posts its one comment at a
//! human, and never reaches for `--admin`, `--auto`, or a push.

#![allow(clippy::unwrap_used, clippy::expect_used)]

#[cfg(unix)]
use serde_json::{Value, json};

mod support;

#[cfg(unix)]
use support::pr_sweep::{
    HEAD, app_bot_author, app_bot_pr_list, human_author, human_pr_list, mutations, one_pr_list,
    run_sweep, setup, write_pr_list, write_view,
};

/// The rebase command, spelled once so the assertion and the tool agree.
#[cfg(unix)]
const REBASE: &str = "@dependabot rebase";

/// The head a modelled Dependabot rebase produces.
#[cfg(unix)]
const REBASED_HEAD: &str = "5555555555555555555555555555555555555555";

/// The head the shared fake's `update-branch` handler rewrites from.
///
/// The handler matches this literal, so a fixture that wants the API update to
/// succeed has to start here rather than at the default head.
#[cfg(unix)]
const UPDATABLE_HEAD: &str = "4444444444444444444444444444444444444444";

/// Reads the recorded comment posts (one argv line each).
#[cfg(unix)]
fn comment_posts(root: &std::path::Path) -> Vec<String> {
    std::fs::read_to_string(root.join("comment_posts.txt"))
        .map(|text| {
            text.lines()
                .map(str::to_owned)
                .filter(|line| !line.trim().is_empty())
                .collect()
        })
        .unwrap_or_default()
}

/// Asserts the two things a mutation log must never contain.
#[cfg(unix)]
fn assert_no_forbidden_mutations(root: &std::path::Path) {
    let cmds = mutations(root);
    for forbidden in ["--admin", "--auto", " push"] {
        assert!(
            cmds.iter().all(|cmd| !cmd.contains(forbidden)),
            "the recovery chain must never use {forbidden:?}: {cmds:#?}"
        );
    }
}

/// `update-branch` refused for a dependency bot: exactly one comment, the head
/// moves, the merge is issued against the *new* head.
#[cfg(unix)]
#[test]
fn a_refused_update_asks_the_bot_to_rebase_then_merges_the_new_head() {
    let (dir, bin) = setup();
    let root = dir.path();
    write_pr_list(root, &app_bot_pr_list(6));
    write_view(
        root,
        6,
        &json!({ "author": app_bot_author(), "mergeStateStatus": "BEHIND" }),
    );
    // The base moved, so the branch is behind and the API refuses.
    std::fs::write(root.join("fail_update"), "x").unwrap();
    // The bot reacts to the command by force-pushing a rebased branch.
    std::fs::write(root.join("rebase_rewrites_head"), "x").unwrap();

    let (code, stdout, stderr) = run_sweep(
        root,
        &bin,
        &[
            "pr",
            "sweep",
            "--pr",
            "6",
            "--execute",
            "--merge",
            "--wait",
            "20",
            "--format",
            "json",
        ],
    );
    assert_eq!(
        code,
        Some(0),
        "a recoverable dependency PR must merge: {stderr}\n{stdout}"
    );
    let report: Value = serde_json::from_str(&stdout).expect("JSON report");
    assert_eq!(report["prs"][0]["outcome"], "merged", "{stdout}");
    assert_eq!(report["prs"][0]["branch_update"], "dependabot-rebase");

    // Exactly one comment, carrying exactly the documented command.
    let posts = comment_posts(root);
    assert_eq!(posts.len(), 1, "one rebase comment per PR: {posts:#?}");
    assert!(
        posts[0].contains(&format!("body={REBASE}")),
        "the body is the documented command: {:?}",
        posts[0]
    );
    assert!(
        posts[0].contains("repos/{owner}/{repo}/issues/6/comments"),
        "{:?}",
        posts[0]
    );

    // The merge is pinned to the head the rebase produced, not the stale one.
    let cmds = mutations(root);
    assert!(
        cmds.iter()
            .any(|cmd| *cmd == format!("pr merge 6 --squash --match-head-commit {REBASED_HEAD}")),
        "merge must pin the rebased head: {cmds:#?}"
    );
    assert!(
        cmds.iter()
            .all(|cmd| !cmd.contains(&format!("--match-head-commit {HEAD}"))),
        "the pre-rebase head must never be merged: {cmds:#?}"
    );

    // The update was still attempted first: the fallback is ordered, not
    // opportunistic.
    assert!(
        cmds.iter().any(|cmd| cmd.contains("update-branch")),
        "the API update must be attempted first: {cmds:#?}"
    );
    assert_no_forbidden_mutations(root);
}

/// Same refusal, a Dependabot that never rebases: blocked, one comment, no
/// merge, and the sweep continues to the next PR instead of halting.
#[cfg(unix)]
#[test]
fn a_bot_that_never_rebases_blocks_without_halting_the_sweep() {
    let (dir, bin) = setup();
    let root = dir.path();
    let mut entries = app_bot_pr_list(6);
    entries.as_array_mut().unwrap().extend(
        one_pr_list(7)
            .as_array()
            .unwrap()
            .iter()
            .map(|entry| {
                let mut entry = entry.clone();
                // A clean human PR behind it, so "the sweep continues" is
                // observable as a merge rather than as an absence.
                entry["headRefName"] = json!("topic-7");
                entry
            })
            .collect::<Vec<Value>>(),
    );
    write_pr_list(root, &entries);
    write_view(
        root,
        6,
        &json!({ "author": app_bot_author(), "mergeStateStatus": "BEHIND" }),
    );
    write_view(root, 7, &json!({}));
    std::fs::write(root.join("fail_update"), "x").unwrap();
    // No `rebase_rewrites_head`: the view keeps the stale head.

    let (code, stdout, stderr) = run_sweep(
        root,
        &bin,
        &[
            "pr",
            "sweep",
            "--all",
            "--execute",
            "--merge",
            "--wait",
            "2",
            "--format",
            "json",
        ],
    );
    assert_eq!(
        code,
        Some(0),
        "a blocked PR is not a sweep failure: {stderr}\n{stdout}"
    );
    let report: Value = serde_json::from_str(&stdout).expect("JSON report");
    let rows = report["prs"].as_array().unwrap();
    assert_eq!(rows[0]["number"], 6);
    assert_eq!(
        rows[0]["outcome"], "blocked",
        "an unrebasable branch is blocked, not unknown: {stdout}"
    );
    assert_eq!(rows[0]["reasons"][0]["code"], "behind_unupdatable");
    assert_eq!(rows[0]["branch_update"], "dependabot-rebase");
    assert!(
        rows[0]["next_action"]
            .as_str()
            .unwrap()
            .contains("did not rebase"),
        "the next action names what happened: {}",
        rows[0]["next_action"]
    );
    assert!(report["halted"].is_null(), "no halt: {stdout}");

    // The sweep moved on: PR 7 is still processed and merged.
    assert_eq!(rows[1]["number"], 7);
    assert_eq!(rows[1]["outcome"], "merged", "{stdout}");

    // Exactly one comment, and none of them at the human PR.
    let posts = comment_posts(root);
    assert_eq!(posts.len(), 1, "{posts:#?}");
    assert!(posts[0].contains("/issues/6/comments"), "{:?}", posts[0]);
    let cmds = mutations(root);
    assert!(
        cmds.iter().all(|cmd| !cmd.contains("pr merge 6")),
        "an unrebasable PR is never merged: {cmds:#?}"
    );
    assert_no_forbidden_mutations(root);
}

/// A human author behind its base: zero comments, zero pushes, blocked.
#[cfg(unix)]
#[test]
fn a_human_author_is_never_commented_at_or_pushed() {
    let (dir, bin) = setup();
    let root = dir.path();
    write_pr_list(root, &human_pr_list(8));
    write_view(
        root,
        8,
        &json!({ "author": human_author(), "mergeStateStatus": "BEHIND" }),
    );
    std::fs::write(root.join("fail_update"), "x").unwrap();

    let (code, stdout, stderr) = run_sweep(
        root,
        &bin,
        &[
            "pr",
            "sweep",
            "--pr",
            "8",
            "--execute",
            "--merge",
            "--wait",
            "2",
            "--format",
            "json",
        ],
    );
    assert_eq!(code, Some(0), "{stderr}\n{stdout}");
    let report: Value = serde_json::from_str(&stdout).expect("JSON report");
    assert_eq!(report["prs"][0]["outcome"], "blocked", "{stdout}");
    assert_eq!(report["prs"][0]["reasons"][0]["code"], "behind_unupdatable");
    assert_eq!(report["prs"][0]["branch_update"], "none");
    assert!(
        report["prs"][0]["next_action"]
            .as_str()
            .unwrap()
            .contains("octocat"),
        "the next action names the author who must act: {}",
        report["prs"][0]["next_action"]
    );

    assert!(
        comment_posts(root).is_empty(),
        "a human author is never commented at: {:?}",
        comment_posts(root)
    );
    let cmds = mutations(root);
    assert!(
        cmds.iter().all(|cmd| !cmd.contains("pr merge")),
        "{cmds:#?}"
    );
    assert_no_forbidden_mutations(root);
}

/// When the API update works, the fallback is never used: no comment is posted.
#[cfg(unix)]
#[test]
fn a_successful_update_posts_no_comment() {
    let (dir, bin) = setup();
    let root = dir.path();
    let mut entries = app_bot_pr_list(9);
    entries[0]["headRefOid"] = json!(UPDATABLE_HEAD);
    write_pr_list(root, &entries);
    write_view(
        root,
        9,
        &json!({
            "author": app_bot_author(),
            "mergeStateStatus": "BEHIND",
            "headRefOid": UPDATABLE_HEAD,
        }),
    );
    // `fail_update` is absent, so the API handler rewrites the head itself.

    let (code, stdout, stderr) = run_sweep(
        root,
        &bin,
        &[
            "pr",
            "sweep",
            "--pr",
            "9",
            "--execute",
            "--merge",
            "--wait",
            "20",
            "--format",
            "json",
        ],
    );
    assert_eq!(code, Some(0), "{stderr}\n{stdout}");
    let report: Value = serde_json::from_str(&stdout).expect("JSON report");
    assert_eq!(report["prs"][0]["outcome"], "merged", "{stdout}");
    assert_eq!(report["prs"][0]["branch_update"], "update-branch");
    assert!(
        comment_posts(root).is_empty(),
        "the fallback is not used opportunistically: {:?}",
        comment_posts(root)
    );
    assert_no_forbidden_mutations(root);
}

/// A head that moves between the rebase and the merge keeps the receipt from
/// authorizing a stale head: the merge is refused, not issued.
#[cfg(unix)]
#[test]
fn a_head_that_moves_after_the_rebase_never_merges_a_stale_receipt() {
    let (dir, bin) = setup();
    let root = dir.path();
    write_pr_list(root, &app_bot_pr_list(10));
    write_view(
        root,
        10,
        &json!({ "author": app_bot_author(), "mergeStateStatus": "BEHIND" }),
    );
    std::fs::write(root.join("fail_update"), "x").unwrap();
    // The bot pushes a rebased head, but the branch keeps moving afterwards, so
    // no snapshot ever stabilizes and no receipt can authorize a merge.
    std::fs::write(root.join("always_moving_after_comment"), "x").unwrap();
    let mut moved = support::pr_sweep::default_view(10);
    moved["author"] = app_bot_author();
    moved["mergeStateStatus"] = json!("CLEAN");
    moved["headRefOid"] = json!("9999999999999999999999999999999999999999");
    std::fs::write(root.join("view_move_1.json"), moved.to_string()).unwrap();
    let mut other = support::pr_sweep::default_view(10);
    other["author"] = app_bot_author();
    other["mergeStateStatus"] = json!("CLEAN");
    other["headRefOid"] = json!("8888888888888888888888888888888888888888");
    std::fs::write(root.join("view_move_2.json"), other.to_string()).unwrap();

    let (code, stdout, stderr) = run_sweep(
        root,
        &bin,
        &[
            "pr",
            "sweep",
            "--pr",
            "10",
            "--execute",
            "--merge",
            "--wait",
            "20",
            "--format",
            "json",
        ],
    );
    // A per-PR unknown does not fail the sweep, and never merges.
    assert_eq!(code, Some(0), "{stderr}\n{stdout}");
    let report: Value = serde_json::from_str(&stdout).expect("JSON report");
    assert_ne!(report["prs"][0]["outcome"], "merged", "{stdout}");
    let cmds = mutations(root);
    assert!(
        cmds.iter().all(|cmd| !cmd.contains("pr merge")),
        "a moved head must never be merged: {cmds:#?}"
    );
    assert_eq!(
        comment_posts(root).len(),
        1,
        "the rebase was still requested exactly once"
    );
    assert_no_forbidden_mutations(root);
}

/// `--dry-run` reports the behind state and posts nothing, so an operator can
/// see the plan before authorizing it.
#[cfg(unix)]
#[test]
fn a_dry_run_neither_updates_nor_comments() {
    let (dir, bin) = setup();
    let root = dir.path();
    write_pr_list(root, &app_bot_pr_list(11));
    write_view(
        root,
        11,
        &json!({ "author": app_bot_author(), "mergeStateStatus": "BEHIND" }),
    );

    let (code, stdout, _stderr) = run_sweep(
        root,
        &bin,
        &["pr", "sweep", "--pr", "11", "--format", "json"],
    );
    assert_eq!(code, Some(0), "{stdout}");
    let report: Value = serde_json::from_str(&stdout).expect("JSON report");
    assert_eq!(report["prs"][0]["reasons"][0]["code"], "update_required");
    assert!(
        report["prs"][0]["branch_update"].is_null(),
        "no mechanism ran: {stdout}"
    );
    assert!(comment_posts(root).is_empty(), "{:?}", comment_posts(root));
    assert!(mutations(root).is_empty(), "{:?}", mutations(root));
}
