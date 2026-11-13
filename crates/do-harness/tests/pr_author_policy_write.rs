//! The author-policy *write* boundary (#519).
//!
//! `pr_author_policy.rs` proves who may be merged. This file proves the other
//! half of the contract, and the configuration surface around it: the relaxed
//! policy widens merging and never writing, so a behind branch, an armed
//! auto-merge, a draft, and a hold label are reported rather than acted on — and
//! an unknown or malformed policy value fails closed before a single `gh` call.
//!
//! Every external fixture answers `404` from the collaborator-permission
//! endpoint, which is what makes a human login `Owner::External` (#514).

#![allow(clippy::unwrap_used, clippy::expect_used)]

#[cfg(unix)]
use serde_json::{Value, json};

mod support;

#[cfg(unix)]
use support::pr_sweep::{
    assert_read_only, external_green_pr, human_author, human_pr_list, mutations, permission_calls,
    run_sweep, setup, write_author_policy_config, write_pr_list, write_view,
};

/// The default policy, spelled explicitly so the fixture states what it assumes.
#[cfg(unix)]
const OWNED: &str = "owned";
/// The relaxed policy.
#[cfg(unix)]
const RELAXED: &str = "owned-or-external-green";

/// A behind external branch is reported, never updated: only its author can
/// move it, and the report says so instead of leaving `merge_state_dirty`.
#[cfg(unix)]
#[test]
fn a_behind_external_branch_is_reported_and_never_updated() {
    let (dir, bin) = setup();
    let root = dir.path();
    write_pr_list(root, &human_pr_list(6));
    write_view(
        root,
        6,
        &json!({ "author": human_author(), "mergeStateStatus": "BEHIND" }),
    );
    std::fs::write(root.join("permission_404"), "x").unwrap();

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
            "--author-policy",
            RELAXED,
            "--format",
            "json",
        ],
    );
    assert_eq!(code, Some(0), "{stderr}");
    let report: Value = serde_json::from_str(&stdout).expect("JSON report");
    assert_eq!(report["prs"][0]["outcome"], "skipped", "{stdout}");
    assert_eq!(
        report["prs"][0]["reasons"][0]["code"], "external_behind",
        "{stdout}"
    );
    assert_eq!(mutations(root), [] as [String; 0], "{stdout}");
    assert_read_only(root);
}

/// An armed auto-merge on an external branch is reported, not disarmed: the
/// disarm would be a write on a branch this tool does not own.
#[cfg(unix)]
#[test]
fn an_armed_auto_merge_on_an_external_branch_is_never_disarmed() {
    let (dir, bin) = setup();
    let root = dir.path();
    write_pr_list(root, &human_pr_list(7));
    write_view(
        root,
        7,
        &json!({
            "author": human_author(),
            "autoMergeRequest": { "enabledAt": "2026-10-01T00:00:00Z" },
        }),
    );
    std::fs::write(root.join("permission_404"), "x").unwrap();

    let (code, stdout, stderr) = run_sweep(
        root,
        &bin,
        &[
            "pr",
            "sweep",
            "--pr",
            "7",
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
    assert_eq!(
        report["prs"][0]["reasons"][0]["code"], "external_auto_merge_armed",
        "{stdout}"
    );
    assert_eq!(mutations(root), [] as [String; 0], "{stdout}");
    assert_read_only(root);
}

/// Drafts and hold labels stay hard stops for every author: relaxing who may
/// merge is not a licence to override the author's own decision.
#[cfg(unix)]
#[test]
fn draft_and_hold_stay_hard_stops_under_the_relaxed_policy() {
    // A draft.
    let (dir, bin) = setup();
    let root = dir.path();
    write_pr_list(root, &human_pr_list(8));
    write_view(
        root,
        8,
        &json!({ "author": human_author(), "isDraft": true }),
    );
    std::fs::write(root.join("permission_404"), "x").unwrap();
    let (code, stdout, _) = run_sweep(
        root,
        &bin,
        &[
            "pr",
            "sweep",
            "--pr",
            "8",
            "--execute",
            "--merge",
            "--author-policy",
            RELAXED,
            "--format",
            "json",
        ],
    );
    assert_eq!(code, Some(0));
    let report: Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(report["prs"][0]["reasons"][0]["code"], "draft", "{stdout}");
    assert!(mutations(root).is_empty(), "{stdout}");

    // A hold label.
    let (dir, bin) = setup();
    let root = dir.path();
    write_pr_list(root, &human_pr_list(9));
    write_view(
        root,
        9,
        &json!({ "author": human_author(), "labels": [{ "name": "do-not-merge" }] }),
    );
    std::fs::write(root.join("permission_404"), "x").unwrap();
    let (code, stdout, _) = run_sweep(
        root,
        &bin,
        &[
            "pr",
            "sweep",
            "--pr",
            "9",
            "--execute",
            "--merge",
            "--author-policy",
            RELAXED,
            "--format",
            "json",
        ],
    );
    assert_eq!(code, Some(0));
    let report: Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(report["prs"][0]["reasons"][0]["code"], "held", "{stdout}");
    assert!(mutations(root).is_empty(), "{stdout}");
}

/// An undetermined identity is never merged, under any policy.
#[cfg(unix)]
#[test]
fn an_unreadable_identity_is_never_merged_under_the_relaxed_policy() {
    let (dir, bin) = setup();
    let root = dir.path();
    write_pr_list(root, &human_pr_list(10));
    write_view(root, 10, &json!({ "author": human_author() }));
    // `403`: the permission is unreadable, so the identity is undetermined —
    // not a third party.
    std::fs::write(root.join("permission_fail"), "x").unwrap();

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
            "--author-policy",
            RELAXED,
            "--format",
            "json",
        ],
    );
    assert_eq!(code, Some(0), "{stderr}");
    let report: Value = serde_json::from_str(&stdout).expect("JSON report");
    assert_eq!(report["prs"][0]["outcome"], "skipped", "{stdout}");
    assert_eq!(
        report["prs"][0]["reasons"][0]["code"], "unknown_identity",
        "\"we could not tell whose branch this is\" is not evidence of safety: {stdout}"
    );
    assert_eq!(mutations(root), [] as [String; 0], "{stdout}");
}

/// An unknown policy value is a usage error that reaches no `gh` call at all.
#[cfg(unix)]
#[test]
fn an_unknown_policy_is_a_usage_error_with_no_mutations() {
    let (dir, bin) = setup();
    let root = dir.path();
    external_green_pr(root, 11);

    let (code, _stdout, stderr) = run_sweep(
        root,
        &bin,
        &[
            "pr",
            "sweep",
            "--pr",
            "11",
            "--execute",
            "--merge",
            "--author-policy",
            "bogus",
            "--format",
            "json",
        ],
    );
    assert_eq!(
        code,
        Some(2),
        "an unknown policy is a usage error: {stderr}"
    );
    assert!(
        stderr.contains("bogus"),
        "the diagnostic names the value: {stderr}"
    );
    // The value never reached the inventory, let alone a mutation.
    assert_eq!(mutations(root), [] as [String; 0], "{stderr}");
    assert!(
        !root.join("gh_calls.txt").exists(),
        "no gh subcommand ran at all: {:?}",
        std::fs::read_to_string(root.join("gh_calls.txt"))
    );
}

/// The `[pr] author_policy` key is honoured without a flag, so a repository can
/// declare the policy once.
#[cfg(unix)]
#[test]
fn the_configuration_key_is_honoured_without_a_flag() {
    let (dir, bin) = setup();
    let root = dir.path();
    external_green_pr(root, 12);
    write_author_policy_config(root, RELAXED);

    let (code, stdout, stderr) = run_sweep(
        root,
        &bin,
        &[
            "pr",
            "sweep",
            "--pr",
            "12",
            "--execute",
            "--merge",
            "--format",
            "json",
        ],
    );
    assert_eq!(code, Some(0), "{stderr}\n{stdout}");
    let report: Value = serde_json::from_str(&stdout).expect("JSON report");
    assert_eq!(report["prs"][0]["outcome"], "merged", "{stdout}");
    assert_eq!(report["author_policy"], RELAXED, "{stdout}");
}

/// A malformed `[pr] author_policy` is a configuration error, never a silent
/// fallback to the default.
#[cfg(unix)]
#[test]
fn a_malformed_configuration_value_exits_two() {
    let (dir, bin) = setup();
    let root = dir.path();
    external_green_pr(root, 13);
    write_author_policy_config(root, "bogus");

    let (code, _stdout, stderr) = run_sweep(
        root,
        &bin,
        &[
            "pr",
            "sweep",
            "--pr",
            "13",
            "--execute",
            "--merge",
            "--format",
            "json",
        ],
    );
    assert_eq!(
        code,
        Some(2),
        "a malformed policy must not fall back to a default: {stderr}"
    );
    assert!(mutations(root).is_empty(), "{stderr}");
}

/// The flag narrows a permissive declaration: one invocation can keep the
/// tighter boundary without editing the repository.
#[cfg(unix)]
#[test]
fn the_flag_narrows_a_permissive_declaration() {
    let (dir, bin) = setup();
    let root = dir.path();
    external_green_pr(root, 14);
    write_author_policy_config(root, RELAXED);

    let (code, stdout, stderr) = run_sweep(
        root,
        &bin,
        &[
            "pr",
            "sweep",
            "--pr",
            "14",
            "--execute",
            "--merge",
            "--author-policy",
            OWNED,
            "--format",
            "json",
        ],
    );
    assert_eq!(code, Some(0), "{stderr}");
    let report: Value = serde_json::from_str(&stdout).expect("JSON report");
    assert_eq!(report["prs"][0]["outcome"], "skipped", "{stdout}");
    assert_eq!(report["author_policy"], OWNED, "{stdout}");
    assert_eq!(mutations(root), [] as [String; 0], "{stdout}");
}

/// The relaxed policy is a merge permission, not a write permission: an owned
/// PR and an external PR in the same sweep are treated according to what each
/// needs, and only the owned one may be updated.
#[cfg(unix)]
#[test]
fn the_relaxed_policy_still_updates_only_the_owned_branch() {
    let (dir, bin) = setup();
    let root = dir.path();
    // PR 20: owned and behind (base moved), so the update path runs. An
    // App-authored bot is owned from the server flag alone, so `permission_404`
    // below cannot also make this one external.
    // PR 21: external and behind, so only its author can move it.
    let mut entries = support::pr_sweep::app_bot_pr_list(20);
    entries.as_array_mut().unwrap().extend(
        support::pr_sweep::human_pr_list(21)
            .as_array()
            .unwrap()
            .iter()
            .cloned(),
    );
    write_pr_list(root, &entries);
    write_view(
        root,
        20,
        &json!({
            "author": support::pr_sweep::app_bot_author(),
            "mergeStateStatus": "BEHIND",
            "headRefOid": "4444444444444444444444444444444444444444",
        }),
    );
    write_view(
        root,
        21,
        &json!({ "author": human_author(), "mergeStateStatus": "BEHIND" }),
    );
    std::fs::write(root.join("permission_404"), "x").unwrap();

    let (code, stdout, stderr) = run_sweep(
        root,
        &bin,
        &[
            "pr",
            "sweep",
            "--all",
            "--execute",
            "--merge",
            "--author-policy",
            RELAXED,
            "--wait",
            "20",
            "--format",
            "json",
        ],
    );
    assert_eq!(code, Some(0), "{stderr}\n{stdout}");
    let report: Value = serde_json::from_str(&stdout).expect("JSON report");
    let rows = report["prs"].as_array().unwrap();
    assert_eq!(rows[0]["number"], 20);
    assert_eq!(
        rows[0]["branch_update"], "update-branch",
        "the owned branch is updated: {stdout}"
    );
    assert_eq!(rows[1]["number"], 21);
    assert_eq!(
        rows[1]["reasons"][0]["code"], "external_behind",
        "the external branch is reported, not moved: {stdout}"
    );

    // Only PR 20 was touched.
    let cmds = mutations(root);
    assert!(
        cmds.iter().all(|cmd| !cmd.contains("/pulls/21/")),
        "no write may target the external PR: {cmds:#?}"
    );
    assert!(
        cmds.iter()
            .any(|cmd| cmd.contains("/pulls/20/update-branch")),
        "the owned branch update must still happen: {cmds:#?}"
    );
    assert!(
        permission_calls(root)
            .iter()
            .any(|call| call.contains("octocat")),
        "the external classification consulted permission: {:?}",
        permission_calls(root)
    );
}
