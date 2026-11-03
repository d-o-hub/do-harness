#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::Path;

use super::no_effect::{
    Effect, MergeImpact, commit_sha, effective_change, merge_base, merge_impact,
    merge_impact_for_pr, pr_base_rev, rev_exists,
};

/// Hex length of a SHA-1 object id.
const SHA1_HEX_LEN: usize = 40;

fn git(dir: &Path, args: &[&str]) {
    let output = crate::changes::git_command(dir)
        .args(args)
        .output()
        .expect("spawn git");
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn init_repo(dir: &Path) {
    git(dir, &["init", "-q", "-b", "main"]);
    git(dir, &["config", "user.email", "test@example.com"]);
    git(dir, &["config", "user.name", "Test"]);
}

fn commit_file(dir: &Path, name: &str, content: &str, message: &str) {
    std::fs::write(dir.join(name), content).unwrap();
    git(dir, &["add", name]);
    git(dir, &["commit", "-q", "-m", message]);
}

fn head_sha(dir: &Path) -> String {
    let output = crate::changes::git_command(dir)
        .args(["rev-parse", "HEAD"])
        .output()
        .expect("rev-parse");
    String::from_utf8_lossy(&output.stdout).trim().to_owned()
}

fn git_stdout(dir: &Path, args: &[&str]) -> String {
    let output = crate::changes::git_command(dir)
        .args(args)
        .output()
        .expect("spawn git");
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_owned()
}

fn current_branch(dir: &Path) -> String {
    git_stdout(dir, &["rev-parse", "--abbrev-ref", "HEAD"])
}

fn worktree_status(dir: &Path) -> String {
    git_stdout(dir, &["status", "--porcelain"])
}

#[test]
fn content_change_has_effect() {
    let dir = tempfile::tempdir().unwrap();
    init_repo(dir.path());
    commit_file(dir.path(), "a.txt", "one\n", "base");
    git(dir.path(), &["switch", "-q", "-c", "feature"]);
    commit_file(dir.path(), "a.txt", "two\n", "change");

    assert_eq!(
        effective_change(dir.path(), "main", "feature").unwrap(),
        Effect::HasEffect
    );
}

#[test]
fn empty_commit_has_no_effect() {
    let dir = tempfile::tempdir().unwrap();
    init_repo(dir.path());
    commit_file(dir.path(), "a.txt", "one\n", "base");
    git(dir.path(), &["switch", "-q", "-c", "feature"]);
    git(
        dir.path(),
        &["commit", "-q", "--allow-empty", "-m", "empty"],
    );

    assert_eq!(
        effective_change(dir.path(), "main", "feature").unwrap(),
        Effect::NoEffect
    );
}

#[test]
fn reverted_change_has_no_effect() {
    let dir = tempfile::tempdir().unwrap();
    init_repo(dir.path());
    commit_file(dir.path(), "a.txt", "one\n", "base");
    git(dir.path(), &["switch", "-q", "-c", "feature"]);
    commit_file(dir.path(), "a.txt", "two\n", "change");
    commit_file(dir.path(), "a.txt", "one\n", "revert");

    assert_eq!(
        effective_change(dir.path(), "main", "feature").unwrap(),
        Effect::NoEffect
    );
}

#[test]
fn merge_base_is_branch_point() {
    let dir = tempfile::tempdir().unwrap();
    init_repo(dir.path());
    commit_file(dir.path(), "a.txt", "one\n", "base");
    let base_sha = head_sha(dir.path());
    git(dir.path(), &["switch", "-q", "-c", "feature"]);
    commit_file(dir.path(), "b.txt", "two\n", "feature");

    assert_eq!(merge_base(dir.path(), "main", "feature").unwrap(), base_sha);
}

#[test]
fn missing_revision_is_an_error() {
    let dir = tempfile::tempdir().unwrap();
    init_repo(dir.path());
    commit_file(dir.path(), "a.txt", "one\n", "base");

    assert!(effective_change(dir.path(), "missing", "main").is_err());
    assert!(rev_exists(dir.path(), "main"));
    assert!(!rev_exists(dir.path(), "missing"));
}

#[test]
fn independently_applied_equivalent_patch_has_no_tree_effect() {
    let dir = tempfile::tempdir().unwrap();
    init_repo(dir.path());
    commit_file(dir.path(), "a.txt", "old\n", "base");
    git(dir.path(), &["switch", "-q", "-c", "feature"]);
    commit_file(dir.path(), "a.txt", "new\n", "feature change");
    git(dir.path(), &["switch", "-q", "main"]);
    commit_file(dir.path(), "a.txt", "new\n", "same change upstream");

    // The historical merge-base diff is nonempty: this is the false negative
    // the merge-impact analysis exists to catch.
    assert_eq!(
        effective_change(dir.path(), "main", "feature").unwrap(),
        Effect::HasEffect
    );

    let impact = merge_impact(dir.path(), "main", "feature");
    assert_eq!(impact.verdict, MergeImpact::NoTreeEffect, "{impact:?}");
    assert_eq!(impact.method, "merge-tree");
    assert!(impact.base_verified, "{impact:?}");
    assert_eq!(
        impact.base_sha.as_deref(),
        Some(commit_sha(dir.path(), "main").unwrap().as_str())
    );
    assert_eq!(
        impact.head_sha.as_deref(),
        Some(commit_sha(dir.path(), "feature").unwrap().as_str())
    );
    assert!(impact.evidence.contains("equals base tree"), "{impact:?}");
}

#[test]
fn genuine_edit_has_tree_effect() {
    let dir = tempfile::tempdir().unwrap();
    init_repo(dir.path());
    commit_file(dir.path(), "a.txt", "one\n", "base");
    git(dir.path(), &["switch", "-q", "-c", "feature"]);
    commit_file(dir.path(), "b.txt", "new\n", "add b");

    let impact = merge_impact(dir.path(), "main", "feature");
    assert_eq!(impact.verdict, MergeImpact::TreeEffect, "{impact:?}");
    assert!(
        impact.evidence.contains("differs from base tree"),
        "{impact:?}"
    );
}

#[test]
fn deletion_and_rename_have_tree_effect() {
    let dir = tempfile::tempdir().unwrap();
    init_repo(dir.path());
    commit_file(dir.path(), "a.txt", "one\n", "base");
    git(dir.path(), &["switch", "-q", "-c", "feature"]);
    std::fs::remove_file(dir.path().join("a.txt")).unwrap();
    git(dir.path(), &["add", "-u"]);
    git(dir.path(), &["commit", "-q", "-m", "delete a"]);
    assert_eq!(
        merge_impact(dir.path(), "main", "feature").verdict,
        MergeImpact::TreeEffect
    );

    git(dir.path(), &["switch", "-q", "main"]);
    git(dir.path(), &["switch", "-q", "-c", "renamed"]);
    git(dir.path(), &["mv", "a.txt", "b.txt"]);
    git(dir.path(), &["commit", "-q", "-m", "rename a"]);
    assert_eq!(
        merge_impact(dir.path(), "main", "renamed").verdict,
        MergeImpact::TreeEffect
    );
}

#[test]
fn conflicting_merge_is_unknown() {
    let dir = tempfile::tempdir().unwrap();
    init_repo(dir.path());
    commit_file(dir.path(), "a.txt", "base\n", "base");
    git(dir.path(), &["switch", "-q", "-c", "feature"]);
    commit_file(dir.path(), "a.txt", "feature\n", "feature change");
    git(dir.path(), &["switch", "-q", "main"]);
    commit_file(dir.path(), "a.txt", "main\n", "main change");

    let impact = merge_impact(dir.path(), "main", "feature");
    assert_eq!(impact.verdict, MergeImpact::Unknown, "{impact:?}");
    assert_eq!(impact.evidence, "merge-conflict");
    assert!(
        impact.reason.as_deref().unwrap_or("").contains("conflicts"),
        "{impact:?}"
    );
}

#[test]
fn unavailable_revisions_are_unknown_not_no_effect() {
    let dir = tempfile::tempdir().unwrap();
    init_repo(dir.path());
    commit_file(dir.path(), "a.txt", "one\n", "base");

    let impact = merge_impact(dir.path(), "missing", "main");
    assert_eq!(impact.verdict, MergeImpact::Unknown, "{impact:?}");
    assert!(impact.base_sha.is_none());
    assert!(
        impact
            .reason
            .as_deref()
            .unwrap_or("")
            .contains("unavailable"),
        "{impact:?}"
    );
}

#[test]
fn merge_impact_does_not_mutate_the_worktree_or_branch() {
    let dir = tempfile::tempdir().unwrap();
    init_repo(dir.path());
    commit_file(dir.path(), "a.txt", "old\n", "base");
    git(dir.path(), &["switch", "-q", "-c", "feature"]);
    commit_file(dir.path(), "a.txt", "new\n", "feature change");
    git(dir.path(), &["switch", "-q", "main"]);
    commit_file(dir.path(), "a.txt", "new\n", "same change upstream");

    let branch = current_branch(dir.path());
    assert_eq!(worktree_status(dir.path()), "");
    let before = std::fs::read_to_string(dir.path().join("a.txt")).unwrap();
    let _ = merge_impact(dir.path(), "main", "feature");
    assert_eq!(current_branch(dir.path()), branch);
    assert_eq!(worktree_status(dir.path()), "");
    assert_eq!(
        std::fs::read_to_string(dir.path().join("a.txt")).unwrap(),
        before
    );
}

#[test]
fn pr_base_rev_never_substitutes_a_stale_ref_for_a_known_oid() {
    let dir = tempfile::tempdir().unwrap();
    init_repo(dir.path());
    commit_file(dir.path(), "a.txt", "one\n", "base");

    let missing_oid = "0".repeat(SHA1_HEX_LEN);
    let (rev, warnings, verified) = pr_base_rev(dir.path(), &missing_oid, "main");
    assert!(rev.is_none(), "a known but missing OID must not fall back");
    assert!(!verified);
    assert!(
        warnings.iter().any(|warning| warning.contains("git fetch")),
        "{warnings:?}"
    );

    let (rev, warnings, verified) = pr_base_rev(dir.path(), "", "main");
    assert_eq!(rev.as_deref(), Some("main"));
    assert!(!verified, "an OID-less base is only a labelled fallback");
    assert!(
        warnings
            .iter()
            .any(|warning| warning.contains("may be stale")),
        "{warnings:?}"
    );
}

#[test]
fn pr_merge_impact_uses_the_verified_base_oid() {
    let dir = tempfile::tempdir().unwrap();
    init_repo(dir.path());
    commit_file(dir.path(), "a.txt", "old\n", "base");
    git(dir.path(), &["switch", "-q", "-c", "feature"]);
    commit_file(dir.path(), "a.txt", "new\n", "feature change");
    git(dir.path(), &["switch", "-q", "main"]);
    commit_file(dir.path(), "a.txt", "new\n", "same change upstream");

    let base_oid = commit_sha(dir.path(), "main").unwrap();
    let head = commit_sha(dir.path(), "feature").unwrap();
    let impact = merge_impact_for_pr(dir.path(), &base_oid, "main", &head);
    assert_eq!(impact.verdict, MergeImpact::NoTreeEffect, "{impact:?}");
    assert!(impact.base_verified, "{impact:?}");
    assert_eq!(impact.base, "main");
}

#[test]
fn unverified_pr_base_cannot_prove_no_tree_effect() {
    let dir = tempfile::tempdir().unwrap();
    init_repo(dir.path());
    commit_file(dir.path(), "a.txt", "old\n", "base");
    git(dir.path(), &["switch", "-q", "-c", "feature"]);
    commit_file(dir.path(), "a.txt", "new\n", "feature change");
    git(dir.path(), &["switch", "-q", "main"]);
    commit_file(dir.path(), "a.txt", "new\n", "same change upstream");

    let head = commit_sha(dir.path(), "feature").unwrap();
    let impact = merge_impact_for_pr(dir.path(), "", "main", &head);
    assert_eq!(impact.verdict, MergeImpact::Unknown, "{impact:?}");
    assert!(!impact.base_verified, "{impact:?}");
    assert!(
        impact.reason.as_deref().unwrap_or("").contains("verified"),
        "{impact:?}"
    );
}
