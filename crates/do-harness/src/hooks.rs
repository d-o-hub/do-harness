//! Management of git hooks that run `do-harness verify` before commits and
//! pushes.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};

use crate::hook_script::{
    BinSource, MARKER, commit_msg_body, only_args, resolve_binary, script_body,
};

/// Executable bits (owner read/write/execute) applied to installed hook files.
const OWNER_EXEC_MASK: u32 = 0o111;

/// Finds the `.git` directory (or git-dir) for the repository containing `cwd`.
///
/// Walks up from `cwd`, returning the first directory that contains an entry
/// named `.git` (a directory for a normal checkout, a file for a linked
/// worktree). When no walk-up match is found, falls back to running
/// `git rev-parse --git-dir` from `cwd`, which also supports bare
/// repositories and alternative layouts.
///
/// # Errors
///
/// Returns an error when neither method locates a repository.
pub fn find_git_dir(cwd: &Path) -> Result<PathBuf> {
    let mut current = Some(cwd);
    while let Some(dir) = current {
        let dot_git = dir.join(".git");
        if dot_git.exists() {
            return Ok(dot_git);
        }
        current = dir.parent();
    }

    let output = Command::new("git")
        .args(["rev-parse", "--git-dir"])
        .current_dir(cwd)
        .output()
        .with_context(|| format!("failed to run `git rev-parse` from {}", cwd.display()))?;
    if !output.status.success() {
        bail!(
            "no git repository found from {} (walk-up found no .git and `git rev-parse --git-dir` failed)",
            cwd.display()
        );
    }
    let git_dir = PathBuf::from(String::from_utf8_lossy(&output.stdout).trim());
    Ok(if git_dir.is_absolute() {
        git_dir
    } else {
        cwd.join(git_dir)
    })
}

/// Installs the pre-commit, pre-push, and commit-msg hooks into
/// `git_dir/hooks/`.
///
/// `pre_commit` and `pre_push` are sensor name lists; an empty list means the
/// full suite (no `--only` flags). The `commit-msg` hook needs no sensor
/// arguments and runs the commitlint script against each prepared message. A
/// hook file that already exists without the [`MARKER`] is refused unless
/// `force` is `true`; files carrying the [`MARKER`] are always overwritten.
///
/// # Errors
///
/// Returns an error when `git_dir/hooks/` does not exist, when a foreign hook
/// would be clobbered without `force`, or when a hook file cannot be written.
pub fn install(
    git_dir: &Path,
    pre_commit: &[String],
    pre_push: &[String],
    force: bool,
) -> Result<()> {
    let hooks_dir = git_dir.join("hooks");
    if !hooks_dir.is_dir() {
        bail!(
            "hooks directory not found at {} (expected `git_dir/hooks`)",
            hooks_dir.display()
        );
    }
    write_hook_checked(
        &hooks_dir.join("pre-commit"),
        &script_body(&only_args(pre_commit)),
        force,
    )?;
    write_hook_checked(
        &hooks_dir.join("pre-push"),
        &script_body(&only_args(pre_push)),
        force,
    )?;
    write_hook_checked(&hooks_dir.join("commit-msg"), &commit_msg_body(), force)?;
    Ok(())
}

/// Removes only hook files in `git_dir/hooks/` that contain the [`MARKER`].
///
/// Foreign hook files are left untouched, and missing files or directories do
/// not produce an error.
///
/// # Errors
///
/// Returns an error when a managed hook file cannot be removed.
pub fn uninstall(git_dir: &Path) -> Result<()> {
    let hooks_dir = git_dir.join("hooks");
    for name in ["pre-commit", "pre-push", "commit-msg"] {
        let path = hooks_dir.join(name);
        if let Ok(content) = fs::read_to_string(&path)
            && content.contains(MARKER)
        {
            fs::remove_file(&path)
                .with_context(|| format!("failed to remove {}", path.display()))?;
        }
    }
    Ok(())
}

/// Reports hook installation state.
#[derive(Debug)]
pub struct HookStatus {
    /// Whether the managed pre-commit hook file is present and carries the marker.
    pub pre_commit: bool,
    /// Whether the managed pre-push hook file is present and carries the marker.
    pub pre_push: bool,
    /// Whether the managed commit-msg hook file is present and carries the marker.
    pub commit_msg: bool,
    /// Where the `do-harness` binary resolves from.
    pub binary: BinSource,
}

/// Computes the installation state for `git_dir` given the repo root.
#[must_use]
pub fn status(git_dir: &Path, repo_root: &Path) -> HookStatus {
    HookStatus {
        pre_commit: is_managed(&git_dir.join("hooks/pre-commit")),
        pre_push: is_managed(&git_dir.join("hooks/pre-push")),
        commit_msg: is_managed(&git_dir.join("hooks/commit-msg")),
        binary: resolve_binary(repo_root),
    }
}

/// Returns whether the hook file at `path` exists and contains the [`MARKER`].
fn is_managed(path: &Path) -> bool {
    fs::read_to_string(path).is_ok_and(|content| content.contains(MARKER))
}

/// Writes `body` to `path`, refusing to clobber a foreign hook unless `force`
/// is set; hook files carrying the [`MARKER`] are always overwritten.
fn write_hook_checked(path: &Path, body: &str, force: bool) -> Result<()> {
    if let Ok(existing) = fs::read_to_string(path) {
        let managed = existing.contains(MARKER);
        if !managed && !force {
            bail!(
                "refusing to overwrite foreign hook {} (re-run with --force to replace it)",
                path.display()
            );
        }
    }
    write_hook(path, body)
}

/// Writes `body` to `path` and marks it executable.
fn write_hook(path: &Path, body: &str) -> Result<()> {
    fs::write(path, body).with_context(|| format!("failed to write {}", path.display()))?;
    #[cfg(unix)]
    set_executable(path)?;
    Ok(())
}

/// Adds owner execute permission to the file at `path` (unix only).
#[cfg(unix)]
fn set_executable(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let permissions = fs::metadata(path)
        .with_context(|| format!("failed to stat {}", path.display()))?
        .permissions();
    let mode = permissions.mode() | OWNER_EXEC_MASK;
    fs::set_permissions(path, fs::Permissions::from_mode(mode))
        .with_context(|| format!("failed to chmod {}", path.display()))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

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
}
