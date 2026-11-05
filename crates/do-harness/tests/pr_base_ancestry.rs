//! Acceptance fixtures for `pr update-base` (#468).
//!
//! Each case drives the real binary against a fake `gh`, so the verdicts,
//! exit codes, the pinned update-branch call, the bounded poll, and the dry-run
//! read-only guarantee are all exercised without network access. One stacked
//! case uses a real local git repository to prove the child rebase applies no
//! duplicate parent changes.

#![allow(clippy::unwrap_used, clippy::expect_used)]

#[cfg(unix)]
use serde_json::{Value, json};
#[cfg(unix)]
use std::path::Path;

#[cfg(unix)]
mod support;
#[cfg(unix)]
use support::update_base::fake_gh_router;

#[cfg(unix)]
const HEAD: &str = "2222222222222222222222222222222222222222";
#[cfg(unix)]
const NEW_HEAD: &str = "4444444444444444444444444444444444444444";
#[cfg(unix)]
const BASE: &str = "1111111111111111111111111111111111111111";
#[cfg(unix)]
const NEW_BASE: &str = "5555555555555555555555555555555555555555";

#[cfg(unix)]
fn write_view(root: &Path, pr: u64, overrides: &Value) {
    let mut view = json!({
        "number": pr,
        "baseRefName": "main",
        "baseRefOid": BASE,
        "headRefName": "topic",
        "headRefOid": HEAD,
        "mergeStateStatus": "CLEAN",
        "mergeable": "MERGEABLE",
        "autoMergeRequest": null,
        "reviewDecision": "",
        "state": "OPEN",
        "isDraft": false,
        "labels": [],
        "headRepository": { "nameWithOwner": "acme/widget" },
        "author": { "login": "alice" }
    });
    if let (Some(target), Some(source)) = (view.as_object_mut(), overrides.as_object()) {
        for (key, value) in source {
            target.insert(key.clone(), value.clone());
        }
    }
    std::fs::write(root.join("view.json"), view.to_string()).unwrap();
}

#[cfg(unix)]
fn diverged_for(root: &Path, head: &str) {
    std::fs::write(root.join("compare_diverged_for_head.txt"), head).unwrap();
}

#[cfg(unix)]
fn run_update(root: &Path, bin: &Path, args: &[&str]) -> (Option<i32>, String, String) {
    let path = std::env::var("PATH").unwrap_or_default();
    let output = support::harness(root)
        .args(args)
        .env("PATH", format!("{}:{path}", bin.display()))
        .output()
        .expect("spawn do-harness");
    (
        output.status.code(),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

#[cfg(unix)]
fn mutations(root: &Path) -> String {
    std::fs::read_to_string(root.join("mutations.txt")).unwrap_or_default()
}

#[cfg(unix)]
fn reason_codes(stdout: &str) -> Vec<String> {
    let report: Value = serde_json::from_str(stdout).unwrap_or(Value::Null);
    report["reasons"]
        .as_array()
        .map(|reasons| {
            reasons
                .iter()
                .filter_map(|r| r["code"].as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(unix)]
#[test]
fn up_to_date_reports_exact_revisions() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let bin = fake_gh_router(root);
    write_view(root, 4101, &json!({}));

    let (code, stdout, stderr) = run_update(
        root,
        &bin,
        &["pr", "update-base", "4101", "--format", "json"],
    );
    assert_eq!(code, Some(0), "up-to-date must exit 0: {stderr}\n{stdout}");
    let report: Value = serde_json::from_str(&stdout).expect("JSON report");
    assert_eq!(report["decision"], "UP_TO_DATE");
    assert_eq!(report["base_oid"], BASE);
    assert_eq!(report["head_oid"], HEAD);
    assert_eq!(report["merge_base_oid"], BASE);
    assert_eq!(report["head_contains_base"], true);
    assert_eq!(report["ancestry_decision"], "ahead");
    assert_eq!(report["executed"], false);
}

#[cfg(unix)]
#[test]
fn clean_merge_state_without_ancestry_is_not_up_to_date() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let bin = fake_gh_router(root);
    write_view(root, 4102, &json!({}));
    // GitHub reports CLEAN, but the compare API proves the head lacks the base.
    diverged_for(root, HEAD);

    let (code, stdout, _stderr) = run_update(
        root,
        &bin,
        &["pr", "update-base", "4102", "--format", "json"],
    );
    assert_eq!(code, Some(0));
    let report: Value = serde_json::from_str(&stdout).expect("JSON report");
    assert_eq!(report["decision"], "UPDATE_REQUIRED");
    assert_eq!(report["head_contains_base"], false);
    assert_eq!(report["merge_state_status"], "CLEAN");
}

#[cfg(unix)]
#[test]
fn update_polls_new_head_and_records_both_oids() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let bin = fake_gh_router(root);
    write_view(root, 4103, &json!({}));
    diverged_for(root, HEAD);
    std::fs::write(root.join("planned_new_head.txt"), NEW_HEAD).unwrap();

    let (code, stdout, stderr) = run_update(
        root,
        &bin,
        &["pr", "update-base", "4103", "--execute", "--format", "json"],
    );
    assert_eq!(code, Some(0), "update must succeed: {stderr}\n{stdout}");
    let report: Value = serde_json::from_str(&stdout).expect("JSON report");
    assert_eq!(report["decision"], "UPDATE_REQUIRED");
    assert_eq!(report["executed"], true);
    assert_eq!(report["new_head_oid"], NEW_HEAD);
    assert_eq!(
        report["head_contains_base"], true,
        "post-update ancestry revalidates"
    );
    let mutations = mutations(root);
    assert!(
        mutations.contains("pr update-branch 4103 --expected-head-sha") && mutations.contains(HEAD),
        "update-branch must be pinned to the expected head: {mutations}"
    );
    assert_eq!(mutations.trim().lines().count(), 1, "no retry: {mutations}");
}

#[cfg(unix)]
#[test]
fn update_422_race_blocks_without_retry() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let bin = fake_gh_router(root);
    write_view(root, 4104, &json!({}));
    diverged_for(root, HEAD);
    std::fs::write(root.join("fail_update_branch_422"), "").unwrap();

    let (code, stdout, _stderr) = run_update(
        root,
        &bin,
        &["pr", "update-base", "4104", "--execute", "--format", "json"],
    );
    assert_eq!(code, Some(1), "a 422 race must block");
    let report: Value = serde_json::from_str(&stdout).expect("JSON report");
    assert_eq!(report["decision"], "BLOCKED");
    assert_eq!(report["executed"], false);
    assert!(
        reason_codes(&stdout)
            .iter()
            .any(|c| c == "update_rejected_422")
    );
    let mutations = mutations(root);
    assert_eq!(mutations.trim().lines().count(), 1, "no retry: {mutations}");
}

#[cfg(unix)]
#[test]
fn update_403_blocks_without_retry() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let bin = fake_gh_router(root);
    write_view(root, 4105, &json!({}));
    diverged_for(root, HEAD);
    std::fs::write(root.join("fail_update_branch_403"), "").unwrap();

    let (code, stdout, _stderr) = run_update(
        root,
        &bin,
        &["pr", "update-base", "4105", "--execute", "--format", "json"],
    );
    assert_eq!(code, Some(1), "a 403 must block");
    let report: Value = serde_json::from_str(&stdout).expect("JSON report");
    assert_eq!(report["decision"], "BLOCKED");
    assert!(
        reason_codes(&stdout)
            .iter()
            .any(|c| c == "update_rejected_403")
    );
    assert_eq!(mutations(root).trim().lines().count(), 1, "no retry");
}

#[cfg(unix)]
#[test]
fn bounded_timeout_is_unknown() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let bin = fake_gh_router(root);
    write_view(root, 4106, &json!({}));
    diverged_for(root, HEAD);

    let path = std::env::var("PATH").unwrap_or_default();
    let output = support::harness(root)
        .args(["pr", "update-base", "4106", "--execute", "--format", "json"])
        .env("PATH", format!("{}:{path}", bin.display()))
        .env("DO_HARNESS_PR_UPDATE_BASE_TIMEOUT_SECS", "2")
        .output()
        .expect("spawn do-harness");
    assert_eq!(output.status.code(), Some(2), "a poll timeout is UNKNOWN");
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let report: Value = serde_json::from_str(&stdout).expect("JSON report");
    assert_eq!(report["decision"], "UNKNOWN");
    assert!(reason_codes(&stdout).iter().any(|c| c == "poll_timeout"));
    assert!(mutations(root).contains("update-branch"));
}

#[cfg(unix)]
#[test]
fn dry_run_issues_no_mutation() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let bin = fake_gh_router(root);
    write_view(root, 4107, &json!({}));
    diverged_for(root, HEAD);
    std::fs::write(root.join("planned_new_head.txt"), NEW_HEAD).unwrap();

    // Default mode is read-only even though an update is required.
    let (code, stdout, _stderr) = run_update(
        root,
        &bin,
        &["pr", "update-base", "4107", "--format", "json"],
    );
    assert_eq!(code, Some(0));
    let report: Value = serde_json::from_str(&stdout).expect("JSON report");
    assert_eq!(report["decision"], "UPDATE_REQUIRED");
    assert_eq!(report["executed"], false);
    assert!(mutations(root).is_empty(), "no mutation in default mode");

    // The global --dry-run must also suppress --execute.
    write_view(root, 4108, &json!({}));
    let path = std::env::var("PATH").unwrap_or_default();
    let output = support::harness(root)
        .arg("--dry-run")
        .args(["pr", "update-base", "4108", "--execute", "--format", "json"])
        .env("PATH", format!("{}:{path}", bin.display()))
        .output()
        .expect("spawn do-harness");
    assert_eq!(output.status.code(), Some(0));
    assert!(
        mutations(root).is_empty(),
        "global --dry-run must suppress mutations"
    );
}

#[cfg(unix)]
#[test]
fn second_pr_invalidated_after_first_merges() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let bin = fake_gh_router(root);

    // Before the first PR merges, PR 4110 is up to date against the old base.
    write_view(root, 4110, &json!({}));
    let (code, stdout, _stderr) = run_update(
        root,
        &bin,
        &["pr", "update-base", "4110", "--format", "json"],
    );
    assert_eq!(code, Some(0));
    let report: Value = serde_json::from_str(&stdout).expect("JSON report");
    assert_eq!(report["decision"], "UP_TO_DATE");

    // PR 1 merges: the base moves; PR 4110's head no longer contains it.
    write_view(root, 4110, &json!({"baseRefOid": NEW_BASE}));
    diverged_for(root, HEAD);
    let (code, stdout, _stderr) = run_update(
        root,
        &bin,
        &["pr", "update-base", "4110", "--format", "json"],
    );
    assert_eq!(code, Some(0));
    let report: Value = serde_json::from_str(&stdout).expect("JSON report");
    assert_eq!(report["base_oid"], NEW_BASE);
    assert_eq!(
        report["decision"], "UPDATE_REQUIRED",
        "the prior green result is invalidated"
    );
}

#[cfg(unix)]
#[test]
fn stacked_parent_first_blocks_until_parent_merges() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let bin = fake_gh_router(root);
    write_view(
        root,
        4111,
        &json!({
            "baseRefName": "parent",
            "headRefName": "child"
        }),
    );
    diverged_for(root, HEAD);
    std::fs::write(
        root.join("pr_list.json"),
        json!([{"number": 7, "headRefName": "parent"}]).to_string(),
    )
    .unwrap();

    let (code, stdout, _stderr) = run_update(
        root,
        &bin,
        &["pr", "update-base", "4111", "--execute", "--format", "json"],
    );
    assert_eq!(code, Some(1), "a stacked child must wait for its parent");
    let report: Value = serde_json::from_str(&stdout).expect("JSON report");
    assert_eq!(report["decision"], "BLOCKED");
    assert!(
        reason_codes(&stdout)
            .iter()
            .any(|c| c == "stack_parent_first")
    );
    assert!(mutations(root).is_empty(), "no update-branch, no rebase");
}

#[cfg(unix)]
fn git_out(root: &Path, args: &[&str]) -> String {
    let output = support::git_command(root)
        .args(args)
        .output()
        .expect("spawn git");
    assert!(output.status.success(), "git {args:?} failed: {output:?}");
    String::from_utf8_lossy(&output.stdout).into_owned()
}

#[cfg(unix)]
#[test]
fn stacked_child_rebases_without_duplicate_parent_changes() {
    let root_dir = tempfile::tempdir().unwrap();
    let origin_dir = tempfile::tempdir().unwrap();
    let root = root_dir.path();
    let origin_bare = origin_dir.path().join("origin.git");
    let bin = fake_gh_router(root);

    // Build a stack: main -> parent(P, adds a.txt) -> child(C, adds b.txt).
    support::git(root, &["init", "-b", "main"]);
    support::git(root, &["config", "user.email", "t@t"]);
    support::git(root, &["config", "user.name", "t"]);
    std::fs::write(root.join("f.txt"), "f").unwrap();
    support::git(root, &["add", "f.txt"]);
    support::git(root, &["commit", "-m", "M0"]);
    support::git(root, &["init", "--bare", origin_bare.to_str().unwrap()]);
    support::git(
        root,
        &["remote", "add", "origin", origin_bare.to_str().unwrap()],
    );
    support::git(root, &["push", "-u", "origin", "main"]);

    support::git(root, &["checkout", "-b", "parent"]);
    std::fs::write(root.join("a.txt"), "a").unwrap();
    support::git(root, &["add", "a.txt"]);
    support::git(root, &["commit", "-m", "parent change"]);

    support::git(root, &["checkout", "-b", "child"]);
    std::fs::write(root.join("b.txt"), "b").unwrap();
    support::git(root, &["add", "b.txt"]);
    support::git(root, &["commit", "-m", "child change"]);

    // The parent is squash-merged into main: main now contains a.txt as a new commit.
    support::git(root, &["checkout", "main"]);
    std::fs::write(root.join("a.txt"), "a").unwrap();
    support::git(root, &["add", "a.txt"]);
    support::git(root, &["commit", "-m", "M1 integrate parent squash"]);
    support::git(root, &["push", "origin", "main", "parent", "child"]);
    support::git(root, &["checkout", "main"]);

    write_view(
        root,
        4112,
        &json!({
            "baseRefName": "parent",
            "headRefName": "child"
        }),
    );
    diverged_for(root, HEAD);

    let (code, stdout, stderr) = run_update(
        root,
        &bin,
        &["pr", "update-base", "4112", "--execute", "--format", "json"],
    );
    assert_eq!(
        code,
        Some(0),
        "stacked rebase must succeed: {stderr}\n{stdout}"
    );
    let report: Value = serde_json::from_str(&stdout).expect("JSON report");
    assert_eq!(report["decision"], "UPDATE_REQUIRED");
    assert_eq!(report["executed"], true);
    let new_head = report["new_head_oid"].as_str().expect("new head SHA");
    assert_eq!(new_head.len(), 40);

    // The child now sits on origin/main with only its own change, no parent dup.
    let subjects = git_out(root, &["log", "--format=%s", "origin/main..origin/child"]);
    assert!(
        !subjects.lines().any(|line| line == "parent change"),
        "parent commits must not be duplicated: {subjects}"
    );
    let changed = git_out(
        root,
        &["diff", "--name-only", "origin/main", "origin/child"],
    );
    assert_eq!(
        changed.trim(),
        "b.txt",
        "only the child's own change remains"
    );
    let child_head = git_out(root, &["rev-parse", "origin/child"]);
    assert_eq!(
        child_head.trim(),
        new_head,
        "origin/child reflects the rebased head"
    );
}
