//! Integration coverage for `do-harness cleanup`.
//!
//! Every case runs the real binary against a real git repository with a fake
//! `gh` on `PATH`: the command deletes directories, so the proof and the guards
//! are tested against git's own state rather than a model of it.

#![cfg(unix)]
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::Path;
use std::process::Command;

use serde_json::Value;

mod support;

use support::cleanup::{Fixture, candidate, full_page_of_unrelated_merges, git, merged_pr};

#[test]
fn a_merged_worktree_is_proven_by_its_pr_and_cleaned() {
    let fixture = Fixture::new();
    let worktree = fixture.worktree("merged");
    let head = fixture.head("merged");
    fixture.merge("merged");
    fixture.prs("[]", &merged_pr(552, &head));

    let (code, document, stderr) = fixture.json(&["cleanup", "--format", "json"]);
    assert_eq!(code, Some(0), "{stderr}\n{document}");
    let candidate = candidate(&document, &worktree);
    assert_eq!(candidate["verdict"], "cleaned", "{document}");
    assert_eq!(candidate["merge_proof"]["kind"], "pr-merged");
    assert_eq!(candidate["merge_proof"]["pr"], 552);
    assert!(candidate["reclaimed_bytes"].as_u64().unwrap() >= 4096);
    assert!(!worktree.join("target").exists(), "build tree must be gone");
    assert!(
        !git(&fixture.root, &["worktree", "list"]).contains(worktree.to_str().unwrap()),
        "worktree registration must be gone"
    );
}

#[test]
fn an_unmerged_worktree_is_refused_and_untouched() {
    let fixture = Fixture::new();
    let worktree = fixture.worktree("unmerged");
    fixture.prs("[]", "[]");

    let (code, document, stderr) = fixture.json(&["cleanup", "--format", "json"]);
    assert_eq!(code, Some(0), "a safety refusal is not an error: {stderr}");
    let candidate = candidate(&document, &worktree);
    assert_eq!(candidate["verdict"], "refused");
    assert_eq!(candidate["reason"], "unmerged");
    assert!(worktree.join("target/debug/blob.rlib").exists());
}

#[test]
fn a_dirty_worktree_is_refused() {
    let fixture = Fixture::new();
    let worktree = fixture.worktree("dirty");
    let head = fixture.head("dirty");
    fixture.merge("dirty");
    fixture.prs("[]", &merged_pr(1, &head));
    std::fs::write(worktree.join("scratch.txt"), "uncommitted\n").unwrap();

    let (code, document, _stderr) = fixture.json(&["cleanup", "--format", "json"]);
    assert_eq!(code, Some(0));
    assert_eq!(candidate(&document, &worktree)["reason"], "dirty");
    assert!(worktree.join("target").exists());
}

#[test]
fn a_locked_worktree_is_refused_with_its_reason() {
    let fixture = Fixture::new();
    let worktree = fixture.worktree("locked");
    let head = fixture.head("locked");
    fixture.merge("locked");
    fixture.prs("[]", &merged_pr(2, &head));
    git(
        &fixture.root,
        &[
            "worktree",
            "lock",
            "--reason",
            "network share",
            worktree.to_str().unwrap(),
        ],
    );

    let (code, document, _stderr) = fixture.json(&["cleanup", "--format", "json"]);
    assert_eq!(code, Some(0));
    assert_eq!(candidate(&document, &worktree)["reason"], "locked");
    assert!(worktree.join("target").exists());
}

#[test]
fn a_held_cargo_lock_refuses_the_build_tree() {
    let fixture = Fixture::new();
    let worktree = fixture.worktree("busy");
    let head = fixture.head("busy");
    fixture.merge("busy");
    fixture.prs("[]", &merged_pr(3, &head));
    let lock = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(worktree.join("target/debug/.cargo-lock"))
        .unwrap();
    fs2::FileExt::lock_exclusive(&lock).unwrap();

    let (code, document, _stderr) = fixture.json(&["cleanup", "--format", "json"]);
    assert_eq!(code, Some(0));
    assert_eq!(candidate(&document, &worktree)["reason"], "in_use");
    assert!(worktree.join("target/debug/blob.rlib").exists());
    fs2::FileExt::unlock(&lock).unwrap();
}

#[test]
fn a_master_default_repository_resolves_its_base_and_cleans() {
    // Regression: the base was hardcoded to `origin/main`, so in a repository
    // whose remote default branch is `master` the base never resolved and every
    // candidate was reported as an error-class `stale_base` — the command was
    // unusable there, not merely conservative.
    let fixture = Fixture::on_default_branch("master");
    let worktree = fixture.worktree("topic");
    let head = fixture.head("topic");
    fixture.merge("topic");
    fixture.prs("[]", &merged_pr(8, &head));

    let (code, document, stderr) = fixture.json(&["cleanup", "--format", "json"]);
    assert_eq!(code, Some(0), "{stderr}\n{document}");
    assert_eq!(document["origin_main"], "origin/master");
    let candidate = candidate(&document, &worktree);
    assert_eq!(candidate["verdict"], "cleaned", "{document}");
    assert_eq!(candidate["merge_proof"]["kind"], "pr-merged");
    assert!(!worktree.join("target").exists());
}

#[test]
fn a_live_compiler_in_the_worktree_refuses_the_build_tree() {
    // Regression: the first implementation tested `cwd.starts_with(target)`, but
    // neither cargo nor rustc changes directory into the build tree (cargo runs
    // in the worktree root and passes `--out-dir <target>/debug/deps`), so the
    // probe could never fire and this exact race went unguarded.
    let fixture = Fixture::new();
    let worktree = fixture.worktree("compiling");
    let head = fixture.head("compiling");
    fixture.merge("compiling");
    fixture.prs("[]", &merged_pr(7, &head));

    let sleeper = fixture.dir.path().join("bin").join("cargo");
    std::fs::copy("/bin/sleep", &sleeper).unwrap();
    let mut child = Command::new(&sleeper)
        .arg("60")
        .current_dir(&worktree)
        .spawn()
        .expect("spawn fake cargo");

    let (code, document, _stderr) = fixture.json(&["cleanup", "--format", "json"]);
    let _ = child.kill();
    let _ = child.wait();
    assert_eq!(code, Some(0));
    assert_eq!(candidate(&document, &worktree)["reason"], "in_use");
    assert!(worktree.join("target/debug/blob.rlib").exists());
}

#[test]
fn an_explicit_target_dir_is_only_adopted_by_the_worktree_that_owns_it() {
    // Regression: `--target-dir` is a global flag, and the resolver took the
    // first existing path for every candidate. Two worktrees then claimed the
    // same directory, reported each other's size, and both tried to delete it.
    let fixture = Fixture::new();
    let first = fixture.worktree("first");
    let second = fixture.worktree("second");
    for branch in ["first", "second"] {
        let head = fixture.head(branch);
        fixture.merge(branch);
        fixture.prs("[]", &merged_pr(9, &head));
    }

    let (code, document, stderr) = fixture.json(&[
        "cleanup",
        "--dry-run",
        "--target-dir",
        first.join("target").to_str().unwrap(),
        "--format",
        "json",
    ]);
    assert_eq!(code, Some(0), "{stderr}\n{document}");
    assert_eq!(
        candidate(&document, &first)["target_dir"],
        first.join("target").display().to_string()
    );
    assert_eq!(
        candidate(&document, &second)["target_dir"],
        second.join("target").display().to_string(),
        "the second worktree must keep its own build tree"
    );
}

#[test]
fn a_truncated_merged_page_fails_closed_instead_of_claiming_proof_stale() {
    // Regression: the lookup requested only 10 merged pull requests. When a
    // long-lived branch name had more, the match could be past the cut, and the
    // code reported `proof_stale` — asserting the name was reused when the query
    // had simply not looked far enough. A full page now means "unknown".
    let fixture = Fixture::new();
    let worktree = fixture.worktree("longlived");
    let head = fixture.head("longlived");
    fixture.merge("longlived");
    fixture.prs("[]", &full_page_of_unrelated_merges(&head));

    let (code, document, _stderr) = fixture.json(&["cleanup", "--format", "json"]);
    assert_eq!(code, Some(1), "unproven is error-class: {document}");
    assert_eq!(candidate(&document, &worktree)["reason"], "unproven");
    assert!(worktree.join("target").exists(), "nothing may be reclaimed");
}

#[test]
fn a_reused_branch_name_is_proof_stale() {
    let fixture = Fixture::new();
    let worktree = fixture.worktree("reused");
    fixture.merge("reused");
    fixture.prs("[]", &merged_pr(4, &"0".repeat(40)));

    let (code, document, _stderr) = fixture.json(&["cleanup", "--format", "json"]);
    assert_eq!(code, Some(0));
    assert_eq!(candidate(&document, &worktree)["reason"], "proof_stale");
    assert!(worktree.join("target").exists());
}

#[test]
fn a_missing_gh_fails_closed() {
    let fixture = Fixture::new();
    let worktree = fixture.worktree("unproven");
    let head = fixture.head("unproven");
    fixture.merge("unproven");
    fixture.prs("[]", &merged_pr(5, &head));

    let path = std::env::var("PATH").unwrap_or_default();
    let output = support::harness(&fixture.root)
        .args(["cleanup", "--format", "json"])
        .env("PATH", format!("{}:{path}", fixture.gh.display()))
        .env("GH_MOCK", fixture.mock())
        .env("GH_FAIL", "1")
        .output()
        .expect("spawn do-harness");
    assert_eq!(output.status.code(), Some(1), "unproven is error-class");
    let document: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(candidate(&document, &worktree)["reason"], "unproven");
    assert!(worktree.join("target").exists());
}

#[test]
fn a_dry_run_reclaims_nothing_and_repeats_itself() {
    let fixture = Fixture::new();
    let worktree = fixture.worktree("dry");
    let head = fixture.head("dry");
    fixture.merge("dry");
    fixture.prs("[]", &merged_pr(6, &head));

    let (code, first, _) = fixture.json(&["cleanup", "--dry-run", "--format", "json"]);
    assert_eq!(code, Some(0));
    let (_, second, _) = fixture.json(&["cleanup", "--dry-run", "--format", "json"]);
    assert_eq!(candidate(&first, &worktree)["verdict"], "eligible");
    assert!(worktree.join("target").exists());
    assert_eq!(normalize(&first), normalize(&second));
}

/// Removes the two fields a dry run may legitimately vary.
fn normalize(document: &Value) -> Value {
    let mut document = document.clone();
    document["generated_at_unix"] = Value::from(0);
    if let Some(candidates) = document["candidates"].as_array_mut() {
        for candidate in candidates {
            candidate["duration_ms"] = Value::from(0);
        }
    }
    document
}

#[test]
fn keep_retains_the_most_recently_used_tree() {
    let fixture = Fixture::new();
    let older = fixture.worktree("older");
    let newer = fixture.worktree("newer");
    fixture.merge("older");
    fixture.merge("newer");
    // Both branches are ancestors of the default branch, so the local fallback
    // proves them without a pull request; the fixture then sets distinct mtimes.
    fixture.prs("[]", "[]");
    set_modified(&older.join("target"), 1_000_000);
    set_modified(&newer.join("target"), 2_000_000);

    let (code, document, _stderr) = fixture.json(&["cleanup", "--keep", "1", "--format", "json"]);
    assert_eq!(code, Some(0));
    assert_eq!(candidate(&document, &older)["reason"], "kept");
    assert_eq!(candidate(&document, &newer)["verdict"], "cleaned");
    assert!(older.join("target").exists());
    assert!(!newer.join("target").exists());
}

/// Sets a directory's modification time to `seconds` after the Unix epoch.
fn set_modified(path: &Path, seconds: u64) {
    let time = std::time::UNIX_EPOCH + std::time::Duration::from_secs(seconds);
    std::fs::File::open(path)
        .unwrap()
        .set_modified(time)
        .unwrap();
}

#[test]
fn the_main_worktree_is_protected() {
    let fixture = Fixture::new();
    fixture.prs("[]", "[]");
    let root = fixture.root.display().to_string();
    let (code, document, _stderr) =
        fixture.json(&["cleanup", "--worktree", &root, "--format", "json"]);
    assert_eq!(code, Some(0));
    assert_eq!(candidate(&document, &fixture.root)["reason"], "protected");
}
