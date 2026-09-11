//! Management of git hooks that run `do-harness verify` before commits and
//! pushes.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use crate::hook_script::{
    BinSource, MARKER, commit_msg_body, only_args, resolve_binary, script_body,
};

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

    let output = crate::changes::git_command(cwd)
        .args(["rev-parse", "--git-dir"])
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
    crate::fs_perm::set_owner_exec(path)?;
    Ok(())
}

#[cfg(test)]
mod tests;
