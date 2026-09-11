//! Merge-base tree comparison: does a revision introduce any effective change?

use std::path::Path;

use anyhow::{Context, Result, bail};

use crate::changes::git_command;

/// Effective-change verdict for a base/head pair.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Effect {
    /// Head contains no tree delta beyond the merge base.
    NoEffect,
    /// Head differs from the merge base.
    HasEffect,
}

impl Effect {
    /// Whether an effective change remains.
    #[must_use]
    pub fn has_effect(self) -> bool {
        matches!(self, Self::HasEffect)
    }
}

/// Returns the merge base of `base` and `head`.
///
/// # Errors
///
/// Returns an error when either revision is missing or git cannot compute
/// the merge base.
pub fn merge_base(root: &Path, base: &str, head: &str) -> Result<String> {
    let output = git_command(root)
        .args(["merge-base", base, head])
        .output()
        .context("failed to run git merge-base")?;
    if !output.status.success() {
        bail!(
            "git merge-base {base} {head} failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    let sha = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if sha.is_empty() {
        bail!("git merge-base {base} {head} returned no commit");
    }
    Ok(sha)
}

/// Whether `head` introduces any tree delta beyond the merge base.
///
/// # Errors
///
/// Returns an error when either revision is missing or git cannot compare the
/// trees; callers must treat errors as "cannot determine", never as "no effect".
pub fn effective_change(root: &Path, base: &str, head: &str) -> Result<Effect> {
    let merge_commit = merge_base(root, base, head)?;
    let output = git_command(root)
        .args(["diff", "--quiet", &merge_commit, head, "--"])
        .output()
        .context("failed to run git diff")?;
    match output.status.code() {
        Some(0) => Ok(Effect::NoEffect),
        Some(1) => Ok(Effect::HasEffect),
        _ => bail!(
            "git diff --quiet {merge_commit} {head} failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ),
    }
}

/// Resolves `rev` to a full commit SHA.
///
/// # Errors
///
/// Returns an error when `rev` does not resolve to a commit.
pub fn commit_sha(root: &Path, rev: &str) -> Result<String> {
    let output = git_command(root)
        .args(["rev-parse", "--verify", &format!("{rev}^{{commit}}")])
        .output()
        .context("failed to run git rev-parse")?;
    if !output.status.success() {
        bail!(
            "git rev-parse {rev} failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    let sha = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if sha.is_empty() {
        bail!("git rev-parse {rev} returned no commit");
    }
    Ok(sha)
}

/// Whether `rev` resolves to a commit object in the repository.
#[must_use]
pub fn rev_exists(root: &Path, rev: &str) -> bool {
    git_command(root)
        .args(["cat-file", "-e", &format!("{rev}^{{commit}}")])
        .output()
        .is_ok_and(|out| out.status.success())
}
