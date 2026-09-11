#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;

/// Creates a fake repository layout (`<root>/.git/hooks`) and returns the
/// `.git` directory path; the owning `TempDir` keeps the layout alive.
fn fake_git_dir() -> (tempfile::TempDir, PathBuf) {
    let temp = tempfile::tempdir().unwrap();
    let git_dir = temp.path().join(".git");
    fs::create_dir_all(git_dir.join("hooks")).unwrap();
    (temp, git_dir)
}

fn hook_path(git_dir: &Path, name: &str) -> PathBuf {
    git_dir.join("hooks").join(name)
}

fn read(path: &Path) -> String {
    fs::read_to_string(path).unwrap()
}

fn write(path: &Path, content: &str) {
    fs::write(path, content).unwrap();
}

#[test]
fn install_writes_both_hooks_with_expected_arguments() {
    let (_temp, git_dir) = fake_git_dir();
    let pre_commit = vec!["fmt".to_string(), "loc".to_string()];
    let pre_push: Vec<String> = Vec::new();

    install(&git_dir, &pre_commit, &pre_push, false).unwrap();

    let pre_commit_body = read(&hook_path(&git_dir, "pre-commit"));
    assert!(pre_commit_body.contains(MARKER));
    assert!(pre_commit_body.contains("#!/usr/bin/env bash"));
    assert!(pre_commit_body.contains("verify --fail-fast --record --only fmt --only loc"));
    assert!(pre_commit_body.contains("cargo build --release -p do-harness"));

    let pre_push_body = read(&hook_path(&git_dir, "pre-push"));
    assert!(pre_push_body.contains(MARKER));
    assert!(pre_push_body.contains("verify --fail-fast --record"));
    assert!(!pre_push_body.contains("--only"));

    let commit_msg_body_text = read(&hook_path(&git_dir, "commit-msg"));
    assert!(commit_msg_body_text.contains(MARKER));
    assert!(commit_msg_body_text.contains("scripts/check-commitlint.sh"));
    assert!(commit_msg_body_text.contains("--message \"$1\""));
    assert!(!commit_msg_body_text.contains("verify --fail-fast"));
}

#[cfg(unix)]
#[test]
fn installed_hooks_are_executable() {
    use std::os::unix::fs::PermissionsExt;
    let (_temp, git_dir) = fake_git_dir();
    let pre_commit: Vec<String> = Vec::new();
    let pre_push: Vec<String> = Vec::new();

    install(&git_dir, &pre_commit, &pre_push, false).unwrap();

    let mode = fs::metadata(hook_path(&git_dir, "pre-commit"))
        .unwrap()
        .permissions()
        .mode();
    assert_ne!(mode & OWNER_EXEC_MASK, 0);
}

#[test]
fn install_refuses_foreign_hook_without_force() {
    let (_temp, git_dir) = fake_git_dir();
    let foreign = "#!/bin/sh\necho 'my own pre-commit hook'\n";
    write(&hook_path(&git_dir, "pre-commit"), foreign);
    let pre_commit: Vec<String> = Vec::new();
    let pre_push: Vec<String> = Vec::new();

    let result = install(&git_dir, &pre_commit, &pre_push, false);

    assert!(result.is_err());
    assert_eq!(read(&hook_path(&git_dir, "pre-commit")), foreign);
    assert!(!hook_path(&git_dir, "pre-push").exists());

    install(&git_dir, &pre_commit, &pre_push, true).unwrap();

    assert!(read(&hook_path(&git_dir, "pre-commit")).contains(MARKER));
    assert!(read(&hook_path(&git_dir, "pre-push")).contains(MARKER));
    assert!(read(&hook_path(&git_dir, "commit-msg")).contains(MARKER));
}

#[test]
fn install_overwrites_marker_hook_without_force() {
    let (_temp, git_dir) = fake_git_dir();
    write(
        &hook_path(&git_dir, "pre-commit"),
        &format!("{MARKER}\n# stale managed content\n"),
    );
    let pre_commit = vec!["fmt".to_string()];
    let pre_push: Vec<String> = Vec::new();

    install(&git_dir, &pre_commit, &pre_push, false).unwrap();

    let body = read(&hook_path(&git_dir, "pre-commit"));
    assert!(body.contains("verify --fail-fast --record --only fmt"));
    assert!(!body.contains("stale"));
}

#[test]
fn uninstall_removes_managed_hooks_and_keeps_foreign() {
    let (_temp, git_dir) = fake_git_dir();
    let pre_commit: Vec<String> = Vec::new();
    let pre_push: Vec<String> = Vec::new();
    install(&git_dir, &pre_commit, &pre_push, false).unwrap();
    let foreign = "#!/bin/sh\necho 'my own pre-push hook'\n";
    write(&hook_path(&git_dir, "pre-push"), foreign);

    uninstall(&git_dir).unwrap();

    assert!(!hook_path(&git_dir, "pre-commit").exists());
    assert!(!hook_path(&git_dir, "commit-msg").exists());
    assert_eq!(read(&hook_path(&git_dir, "pre-push")), foreign);
    uninstall(&git_dir).unwrap();
}

#[test]
fn find_git_dir_discovers_git_root_from_nested_directory() {
    let temp = tempfile::tempdir().unwrap();
    fs::create_dir_all(temp.path().join(".git")).unwrap();
    let nested = temp.path().join("a/b/c");
    fs::create_dir_all(&nested).unwrap();

    let git_dir = find_git_dir(&nested).unwrap();

    assert_eq!(git_dir, temp.path().join(".git"));
}

#[test]
fn find_git_dir_errors_outside_a_repository() {
    let temp = tempfile::tempdir().unwrap();
    let nested = temp.path().join("a/b");
    fs::create_dir_all(&nested).unwrap();

    // The `git rev-parse --git-dir` fallback inherits this process's
    // environment. The usual hardening (setting GIT_DIR to a bogus path
    // and bounding discovery with GIT_CEILING_DIRECTORIES) is unavailable
    // here: `std::env::set_var` is unsafe on edition 2024 and the
    // workspace forbids unsafe code. Instead we rely on the test
    // environment being clean: `tempdir()` lives under `$TMPDIR` (unset,
    // hence `/tmp`), and neither `/tmp` nor `/` contains a `.git` entry,
    // so both the walk-up and the `git` fallback fail deterministically.
    // The message assertion keeps a regression obvious if the environment
    // changes.
    let result = find_git_dir(&nested);

    let error = result.expect_err("no repository should be found");
    assert!(error.to_string().contains("no git repository found from"));
}

#[test]
fn status_reports_no_hooks_for_empty_hooks_dir() {
    let (temp, git_dir) = fake_git_dir();

    let state = status(&git_dir, temp.path());

    assert!(!state.pre_commit);
    assert!(!state.pre_push);
    assert!(!state.commit_msg);
}

#[test]
fn status_reports_installed_hooks() {
    let (temp, git_dir) = fake_git_dir();
    let pre_commit: Vec<String> = Vec::new();
    let pre_push: Vec<String> = Vec::new();
    install(&git_dir, &pre_commit, &pre_push, false).unwrap();

    let state = status(&git_dir, temp.path());

    assert!(state.pre_commit);
    assert!(state.pre_push);
    assert!(state.commit_msg);
}

#[test]
fn status_detects_release_binary() {
    let (temp, git_dir) = fake_git_dir();
    let bin = temp.path().join("target/release/do-harness");
    fs::create_dir_all(bin.parent().unwrap()).unwrap();
    write(&bin, "stub");

    let state = status(&git_dir, temp.path());

    assert!(state.binary.present());
}

#[test]
fn install_errors_when_hooks_directory_is_missing() {
    let temp = tempfile::tempdir().unwrap();
    let git_dir = temp.path().join(".git");
    let pre_commit: Vec<String> = Vec::new();
    let pre_push: Vec<String> = Vec::new();

    let result = install(&git_dir, &pre_commit, &pre_push, false);

    assert!(result.is_err());
}
