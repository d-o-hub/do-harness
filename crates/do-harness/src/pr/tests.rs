#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::Path;
use std::process::Command;

use super::no_effect::{Effect, effective_change, merge_base, rev_exists};

fn git(dir: &Path, args: &[&str]) {
    let mut command = Command::new("git");
    command.current_dir(dir);
    for key in [
        "GIT_DIR",
        "GIT_WORK_TREE",
        "GIT_INDEX_FILE",
        "GIT_OBJECT_DIRECTORY",
        "GIT_ALTERNATE_OBJECT_DIRECTORIES",
        "GIT_CEILING_DIRECTORIES",
        "GIT_NAMESPACE",
        "GIT_PREFIX",
    ] {
        command.env_remove(key);
    }
    let output = command.args(args).output().expect("spawn git");
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
    let output = Command::new("git")
        .current_dir(dir)
        .args(["rev-parse", "HEAD"])
        .output()
        .expect("rev-parse");
    String::from_utf8_lossy(&output.stdout).trim().to_owned()
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
