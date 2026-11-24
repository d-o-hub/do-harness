//! Shared fixtures for the hook lifecycle tests.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;
use std::path::{Path, PathBuf};

use crate::hook_script::MARKER;

use super::super::{install, status};

/// Creates a fake repository layout (`<root>/.git/hooks`) and returns the
/// `.git` directory path; the owning `TempDir` keeps the layout alive.
pub(super) fn fake_git_dir() -> (tempfile::TempDir, PathBuf) {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let init_status = crate::changes::git_command(root)
        .args(["init", "-q"])
        .status()
        .unwrap();
    assert!(init_status.success());
    let cfg_status = crate::changes::git_command(root)
        .args(["config", "core.hooksPath", ".git/hooks"])
        .status()
        .unwrap();
    assert!(cfg_status.success());
    let git_dir = root.join(".git");
    fs::create_dir_all(git_dir.join("hooks")).unwrap();
    (temp, git_dir)
}

pub(super) fn hook_path(git_dir: &Path, name: &str) -> PathBuf {
    git_dir.join("hooks").join(name)
}

pub(super) fn read(path: &Path) -> String {
    fs::read_to_string(path).unwrap()
}

pub(super) fn write(path: &Path, content: &str) {
    fs::write(path, content).unwrap();
}

pub(super) fn create_linked_worktree(temp: &Path) -> (PathBuf, PathBuf) {
    let parent = temp.join("parent");
    let linked = temp.join("linked");
    assert!(
        crate::changes::git_command(temp)
            .args(["init", "-q", "parent"])
            .status()
            .unwrap()
            .success()
    );
    assert!(
        crate::changes::git_command(&parent)
            .args(["config", "core.hooksPath", ".git/hooks"])
            .status()
            .unwrap()
            .success()
    );
    assert!(
        crate::changes::git_command(&parent)
            .args(["commit", "--allow-empty", "-m", "initial", "-q"])
            .env("GIT_AUTHOR_NAME", "t")
            .env("GIT_AUTHOR_EMAIL", "t@t")
            .env("GIT_COMMITTER_NAME", "t")
            .env("GIT_COMMITTER_EMAIL", "t@t")
            .status()
            .unwrap()
            .success()
    );
    assert!(
        crate::changes::git_command(&parent)
            .args(["worktree", "add", "-b", "fixture", linked.to_str().unwrap()])
            .status()
            .unwrap()
            .success()
    );
    (parent, linked)
}

pub(super) fn assert_install_preserves_default_hooks_alias(
    root: &Path,
    git_dir: &Path,
    alias: &str,
) {
    let git_status = crate::changes::git_command(root)
        .args(["config", "core.hooksPath", alias])
        .status()
        .unwrap();
    assert!(git_status.success());

    for _ in 0..2 {
        let installed_path = install(git_dir, root, &[], &[], false).unwrap();
        assert_eq!(installed_path, root.join(alias));
        for name in ["pre-commit", "pre-push", "commit-msg"] {
            assert!(read(&hook_path(git_dir, name)).contains(MARKER));
            assert_eq!(
                read(&installed_path.join(name)),
                read(&hook_path(git_dir, name))
            );
        }
        assert!(status(git_dir, root).is_all_satisfied());
    }
}
