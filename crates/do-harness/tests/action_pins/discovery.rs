//! Discovery cases for the `action-pins` sensor.
//!
//! Discovery decides which files the gate reads at all, so a boundary that
//! is wrong there is a boundary where a mutable reference is never seen:
//! a nested directory, a new extension, an unstaged file, a deleted file,
//! a failing git command, and an empty inventory.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::Path;
use std::process::{Command, Output};

use crate::fixtures::{git_workspace, run, workflow_with, workspace, write_workflow};
use crate::support;

/// Nested workflow directories are discovered, not only the top level.
#[test]
fn nested_workflows_are_discovered() {
    let dir = workspace();
    write_workflow(
        dir.path(),
        ".github/workflows/nested/pins.yaml",
        &workflow_with("actions/checkout@v4"),
    );

    let (code, output) = run(dir.path());
    assert_eq!(code, 1, "a nested workflow must be checked: {output}");
    assert!(
        output.contains(".github/workflows/nested/pins.yaml:7:"),
        "the nested finding must name its file: {output}"
    );
}

/// A `.yaml` extension is discovered as well as `.yml`.
#[test]
fn yaml_extension_is_discovered() {
    let dir = workspace();
    write_workflow(
        dir.path(),
        ".github/workflows/pins.yaml",
        &workflow_with("actions/checkout@v4"),
    );

    let (code, output) = run(dir.path());
    assert_eq!(code, 1, "a .yaml workflow must be checked: {output}");
    assert!(
        output.contains(".github/workflows/pins.yaml:7:"),
        "the finding must name its file: {output}"
    );
}

/// A file that is not a workflow is not discovered.
#[test]
fn non_workflow_file_is_not_discovered() {
    let dir = workspace();
    write_workflow(
        dir.path(),
        ".github/workflows/README.md",
        "actions/checkout@v4\n",
    );

    let (code, output) = run(dir.path());
    assert_eq!(
        code, 1,
        "a repository without workflows must fail: {output}"
    );
    assert!(
        output.contains("no workflow files found"),
        "the failure must name the empty inventory: {output}"
    );
}

/// An unstaged new workflow is discovered: the file is not in the index yet.
#[test]
fn unstaged_workflow_is_discovered() {
    let dir = git_workspace();
    let root = dir.path();
    write_workflow(
        root,
        ".github/workflows/new.yml",
        &workflow_with("actions/checkout@v4"),
    );

    let (code, output) = run(root);
    assert_eq!(code, 1, "an unstaged workflow must be checked: {output}");
    assert!(
        output.contains(".github/workflows/new.yml:7:"),
        "the finding must name the new file: {output}"
    );
}

/// A linked worktree's `.git` file still selects Git-backed discovery.
#[test]
fn worktree_git_file_is_discovered() {
    let primary = git_workspace();
    let primary_root = primary.path();
    let commit = support::git_command(primary_root)
        .args([
            "-c",
            "user.name=fixture",
            "-c",
            "user.email=fixture@example.com",
            "commit",
            "--allow-empty",
            "-m",
            "chore(fixture): seed worktree",
        ])
        .status()
        .expect("create baseline commit");
    assert!(commit.success(), "the baseline commit must succeed");

    let parent = tempfile::tempdir().expect("worktree parent");
    let worktree = parent.path().join("linked");
    let added = support::git_command(primary_root)
        .args(["worktree", "add", "--quiet", "-b", "action-pins-fixture"])
        .arg(&worktree)
        .arg("HEAD")
        .status()
        .expect("create linked worktree");
    assert!(added.success(), "the linked worktree must be created");

    write_workflow(
        &worktree,
        ".github/workflows/pins.yml",
        &workflow_with("actions/checkout@v4"),
    );
    let (code, output) = run(&worktree);
    assert_eq!(
        code, 1,
        "a workflow in a linked worktree must be checked: {output}"
    );
    assert!(
        output.contains(".github/workflows/pins.yml:7:"),
        "the finding must name the linked-worktree workflow: {output}"
    );
}

/// A deleted workflow is not checked: the file is gone. The fixture stages
/// the file before deleting it, so the deletion is a real index change
/// rather than a file that was never tracked.
#[test]
fn deleted_workflow_is_not_checked() {
    let dir = git_workspace();
    let root = dir.path();
    write_workflow(
        root,
        ".github/workflows/gone.yml",
        &workflow_with("actions/checkout@v4"),
    );
    stage(root, ".github/workflows/gone.yml");
    std::fs::remove_file(root.join(".github/workflows/gone.yml")).expect("remove workflow");

    let (code, output) = run(root);
    assert_ne!(code, 0, "a repository with no workflow must fail: {output}");
    assert!(
        output.contains("no workflow files found"),
        "the failure must name the empty discovery: {output}"
    );
}

/// A deleted workflow does not hide the workflows that remain.
#[test]
fn deleted_workflow_leaves_the_rest_checked() {
    let dir = git_workspace();
    let root = dir.path();
    write_workflow(
        root,
        ".github/workflows/kept.yml",
        &workflow_with("actions/checkout@v4"),
    );
    write_workflow(
        root,
        ".github/workflows/gone.yml",
        &workflow_with("actions/checkout@v4"),
    );
    stage(root, ".github/workflows/gone.yml");
    std::fs::remove_file(root.join(".github/workflows/gone.yml")).expect("remove workflow");

    let (code, output) = run(root);
    assert_eq!(
        code, 1,
        "the remaining workflow must still be checked: {output}"
    );
    assert!(
        output.contains(".github/workflows/kept.yml:7:"),
        "the remaining workflow must be reported: {output}"
    );
    assert!(
        !output.contains("gone.yml"),
        "the deleted workflow must not be checked: {output}"
    );
}

/// A discovery command that fails fails the check closed.
#[test]
fn discovery_failure_fails_closed() {
    let dir = workspace();
    let root = dir.path();
    // A `.git` directory that is not a repository: `git ls-files` fails, and a
    // checker that silently checked nothing would report a vacuous pass.
    std::fs::create_dir_all(root.join(".git")).expect("create .git");
    write_workflow(
        root,
        ".github/workflows/pins.yml",
        &workflow_with("actions/checkout@v4"),
    );

    let (code, output) = run(root);
    assert_eq!(code, 1, "a discovery failure must fail closed: {output}");
    assert!(
        output.contains("git discovery failed"),
        "the failure must name the discovery error: {output}"
    );
    assert!(
        output.contains("FINDINGS: 1"),
        "the discovery failure must be counted: {output}"
    );
}

/// A repository without any workflow file fails: a gate that checks nothing
/// is not a pass.
#[test]
fn zero_workflow_files_fail() {
    let dir = workspace();

    let (code, output) = run(dir.path());
    assert_eq!(code, 1, "an empty inventory must fail: {output}");
    assert!(
        output.contains("no workflow files found"),
        "the failure must name the empty inventory: {output}"
    );
    assert!(
        output.contains("FINDINGS: 1"),
        "the empty inventory must be counted: {output}"
    );
}

/// A repository without a `.github/workflows` directory at all fails: the
/// discovery root is missing, which is a configuration error, not a pass.
#[test]
fn missing_workflow_directory_fails() {
    let dir = tempfile::tempdir().expect("create temp dir");

    let (code, output) = run(dir.path());
    assert_eq!(code, 1, "a missing workflow directory must fail: {output}");
    assert!(
        output.contains("no .github/workflows directory found"),
        "the failure must name the missing directory: {output}"
    );
}

/// A workflow file whose name carries leading or trailing whitespace is a
/// real file: trimming the name would look for a path that does not exist
/// and the reference would never be checked.
#[test]
fn whitespace_in_file_name_is_preserved() {
    let dir = git_workspace();
    write_workflow(
        dir.path(),
        ".github/workflows/ spaced .yml",
        &workflow_with("actions/checkout@v4"),
    );

    let (code, output) = run(dir.path());
    assert_eq!(
        code, 1,
        "a spaced workflow name must still be checked: {output}"
    );
    assert!(
        output.contains(".github/workflows/ spaced .yml:7:"),
        "the finding must name the file with its exact bytes: {output}"
    );
}

/// A workflow file whose name contains a newline is discovered through the
/// NUL-delimited git listing and reported on one line, with the newline
/// escaped: a line-delimited listing would split the name into two paths
/// and check neither.
#[cfg(unix)]
#[test]
fn newline_in_file_name_is_discovered_and_escaped() {
    let dir = git_workspace();
    let root = dir.path();
    write_workflow(
        root,
        ".github/workflows/od\nd.yml",
        &workflow_with("actions/checkout@v4"),
    );

    let (code, output) = run(root);
    assert_eq!(
        code, 1,
        "a newline in a file name must still be checked: {output}"
    );
    let finding = output
        .lines()
        .find(|line| line.contains("action reference"))
        .unwrap_or_else(|| panic!("the finding must be reported: {output}"));
    assert!(
        finding.starts_with(".github/workflows/od\\nd.yml:7:"),
        "the finding must name the file with its newline escaped: {finding}"
    );
}

/// An outer repository's `GIT_DIR` must not decide what the checker reads:
/// `--root` names the repository, and the checker's own listing must not
/// be redirected to a different worktree by an inherited environment.
#[test]
fn outer_git_directory_does_not_redirect_discovery() {
    let dir = git_workspace();
    let root = dir.path();
    write_workflow(
        root,
        ".github/workflows/pins.yml",
        &workflow_with("actions/checkout@v4"),
    );

    // The checkout the test runs from is a git repository; point `GIT_DIR` at
    // its actual administrative directory, including when `.git` is a file.
    let outer = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let git_dir = support::git_command(&outer)
        .args(["rev-parse", "--absolute-git-dir"])
        .output()
        .expect("resolve outer git directory");
    assert!(
        git_dir.status.success(),
        "the test checkout must have a git directory"
    );
    let outer_git = String::from_utf8_lossy(&git_dir.stdout).trim().to_owned();

    let mut cmd = Command::new("python3");
    support::isolate_command(&mut cmd);
    cmd.arg(crate::fixtures::checker())
        .arg("--root")
        .arg(root)
        .env("GIT_DIR", &outer_git);
    let output: Output = cmd.output().expect("spawn check-action-pins.py");
    let text = String::from_utf8_lossy(&output.stdout).to_string();

    assert!(
        text.contains(".github/workflows/pins.yml:7:"),
        "the fixture's own workflow must be the one reported: {text}"
    );
    assert!(
        !text.contains("agt-ga-watch"),
        "an outer repository's workflows must not be inspected: {text}"
    );
}

/// Stages a path in the fixture repository, so a later deletion is a real
/// index change rather than a file that was never tracked.
fn stage(root: &std::path::Path, relative: &str) {
    let status = support::git_command(root)
        .args(["add", relative])
        .status()
        .expect("stage path");
    assert!(status.success(), "git add must succeed");
}
