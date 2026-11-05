//! Stacked-PR revalidation (#468).
//!
//! A stacked child's base is another PR's head branch. The parent must merge
//! first; once it has, the child's branch still carries the parent's old
//! commits, which re-appear in the child diff. The child is rebased onto the
//! new default branch and force-pushed only for an authorized own/known-bot
//! branch, mirroring `references/merge-order.md`. The rebase stops on conflict
//! and the result is checked to apply no duplicate parent changes before the
//! push is allowed.
use std::path::Path;

use anyhow::{Context, Result};

use super::gh;
use crate::changes::{current_branch, git_command};

/// Result of a stacked child rebase.
#[derive(Debug, Clone)]
pub enum StackOutcome {
    /// An open parent PR still owns the child's base; merge it first.
    ParentFirst,
    /// The parent branch reference is gone and its old head cannot be proven.
    ParentGone,
    /// The rebase conflicted or would duplicate parent changes; nothing changed.
    Conflict,
    /// The child was rebased and force-pushed to the new head.
    Rebased { new_head: String },
}

/// Rebases a stacked child onto the default branch after its parent merged.
pub fn rebase(
    root: &Path,
    pr: u64,
    head_branch: &str,
    base_branch: &str,
    default_branch: &str,
) -> Result<StackOutcome> {
    let open = gh::open_prs(root)?;
    if open
        .iter()
        .any(|candidate| candidate.head_ref_name == base_branch)
    {
        return Ok(StackOutcome::ParentFirst);
    }

    fetch(root)?;
    let Some(old_parent) = rev(root, &format!("origin/{base_branch}")) else {
        return Ok(StackOutcome::ParentGone);
    };
    if rev(root, &format!("origin/{head_branch}")).is_none() {
        anyhow::bail!("child head branch origin/{head_branch} is missing locally");
    }

    let local = format!("do-harness-update-base-{pr}");
    let previous = current_branch(root);
    git(
        root,
        &["checkout", "-B", &local, &format!("origin/{head_branch}")],
    )?;

    let rebased = git(
        root,
        &[
            "rebase",
            "--onto",
            &format!("origin/{default_branch}"),
            &old_parent,
        ],
    );
    if rebased.is_err() {
        let _ = git(root, &["rebase", "--abort"]);
        restore(root, previous.as_deref());
        cleanup(root, &local);
        return Ok(StackOutcome::Conflict);
    }

    let Some(new_head) = rev(root, "HEAD") else {
        restore(root, previous.as_deref());
        cleanup(root, &local);
        anyhow::bail!("post-rebase HEAD could not be resolved");
    };

    // Prove the rebased head applies none of the parent's changes: the changed
    // path set of the child's own commits must equal the changed path set of
    // the rebased branch against the new base.
    let child_only = diff_name_only(root, &old_parent, &format!("origin/{head_branch}"))?;
    let after = diff_name_only(root, &format!("origin/{default_branch}"), "HEAD")?;
    if child_only != after {
        restore(root, previous.as_deref());
        cleanup(root, &local);
        return Ok(StackOutcome::Conflict);
    }

    let pushed = git(
        root,
        &[
            "push",
            "--force-with-lease",
            "origin",
            &format!("{local}:{head_branch}"),
        ],
    );
    restore(root, previous.as_deref());
    cleanup(root, &local);
    pushed.with_context(|| format!("stacked force-push of {head_branch} failed"))?;

    Ok(StackOutcome::Rebased { new_head })
}

/// Runs `git fetch origin` in `root`.
fn fetch(root: &Path) -> Result<()> {
    git(root, &["fetch", "origin"])
}

/// Runs `git <args>` in `root`, failing on a non-zero exit.
fn git(root: &Path, args: &[&str]) -> Result<()> {
    let output = git_command(root)
        .args(args)
        .output()
        .with_context(|| format!("failed to run git {}", args.join(" ")))?;
    if output.status.success() {
        Ok(())
    } else {
        anyhow::bail!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
}

/// Resolves `rev` to a full commit SHA, or `None` when it does not resolve.
fn rev(root: &Path, rev: &str) -> Option<String> {
    let output = git_command(root)
        .args(["rev-parse", "--verify", "--quiet", rev])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let value = String::from_utf8_lossy(&output.stdout);
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_owned())
}

/// Sorted changed-path set of `a..b`.
fn diff_name_only(root: &Path, a: &str, b: &str) -> Result<String> {
    let output = git_command(root)
        .args(["diff", "--name-only", a, b])
        .output()
        .with_context(|| format!("failed to run git diff --name-only {a} {b}"))?;
    if !output.status.success() {
        anyhow::bail!("git diff --name-only {a} {b} failed");
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let paths: Vec<&str> = text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    Ok(paths.join("\n"))
}

/// Returns to the pre-rebase branch, when one was recorded.
fn restore(root: &Path, previous: Option<&str>) {
    if let Some(branch) = previous {
        let _ = git(root, &["switch", branch]);
    }
}

/// Removes the temporary local branch, ignoring failure.
fn cleanup(root: &Path, local: &str) {
    let _ = git(root, &["branch", "-D", local]);
}
