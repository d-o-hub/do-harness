//! Disposable job workspaces.
//!
//! # Isolation boundary (be explicit)
//!
//! A git worktree isolates the *checkout*: the executor writes into a
//! throwaway directory created from a pinned commit, never into the caller's
//! working tree. It is **not** a process, network, or filesystem sandbox — the
//! executor still runs with the caller's privileges and syscall access. The
//! spec must therefore name the external mechanism that provides that
//! isolation; this CLI records the declaration and never claims to verify it.

use std::path::Path;
use std::process::Command;

use anyhow::Result;

use crate::exit::UsageError;

/// Resolves `revision` to the commit the workspace must be created from.
///
/// # Errors
///
/// Returns a usage error when `root` is not a git repository or the revision
/// does not resolve to a commit.
pub fn resolve_revision(root: &Path, revision: &str) -> Result<String> {
    let expression = format!("{revision}^{{commit}}");
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["rev-parse", "--verify", "--quiet", &expression])
        .output()
        .map_err(|error| anyhow::Error::from(UsageError::wrap(error)))?;
    if !output.status.success() {
        return Err(UsageError::msg(format!(
            "revision '{revision}' does not resolve to a commit in {}",
            root.display()
        ))
        .into());
    }
    let sha = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if sha.is_empty() || !sha.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(UsageError::msg(format!(
            "revision '{revision}' resolved to an unexpected object id"
        ))
        .into());
    }
    Ok(sha)
}

/// Creates the disposable worktree at `workspace` from `sha`.
///
/// An existing workspace is reused: that is the interrupted-job case, where
/// resume must not discard work in progress. Any other existing path is
/// refused rather than overwritten.
///
/// # Errors
///
/// Returns a usage error when the worktree cannot be created.
pub fn prepare(root: &Path, workspace: &Path, sha: &str) -> Result<()> {
    if workspace.is_dir() {
        return Ok(());
    }
    if workspace.exists() {
        return Err(UsageError::msg(format!(
            "job workspace {} exists and is not a directory",
            workspace.display()
        ))
        .into());
    }
    if let Some(parent) = workspace.parent() {
        std::fs::create_dir_all(parent).map_err(|error| {
            anyhow::Error::from(UsageError::msg(format!(
                "failed to create {}: {error}",
                parent.display()
            )))
        })?;
    }
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["worktree", "add", "--detach"])
        .arg(workspace)
        .arg(sha)
        .output()
        .map_err(|error| anyhow::Error::from(UsageError::wrap(error)))?;
    if !output.status.success() {
        return Err(UsageError::msg(format!(
            "failed to create job workspace at {}: {}",
            workspace.display(),
            String::from_utf8_lossy(&output.stderr).trim()
        ))
        .into());
    }
    Ok(())
}

/// Removes a disposable worktree, returning whether it is gone.
///
/// A failed removal is reported rather than ignored, but never turns a
/// verified outcome into a failure: the receipt records that the workspace
/// still exists for cleanup.
#[must_use]
pub fn remove(root: &Path, workspace: &Path) -> bool {
    if !workspace.exists() {
        return true;
    }
    let status = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["worktree", "remove", "--force"])
        .arg(workspace)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
    let removed = status.is_ok_and(|status| status.success());
    if removed {
        let _ = std::fs::remove_dir_all(workspace);
    }
    removed || !workspace.exists()
}
