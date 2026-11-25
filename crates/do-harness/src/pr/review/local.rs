//! Local git plumbing for review inputs: merge-base resolution and diffs.

use std::path::Path;

use anyhow::{Context, Result, bail};

use super::gh;
use super::no_effect;
use crate::changes::git_command;

/// Resolves merge base, current base tip, and head for a PR from the local
/// clone, when present.
pub(super) fn local_pr_refs(root: &Path, view: &gh::PrView) -> Option<(String, String, String)> {
    if !no_effect::rev_exists(root, &view.head_ref_oid) {
        return None;
    }
    for candidate in no_effect::base_refs(&view.base_ref_name) {
        if !no_effect::rev_exists(root, &candidate) {
            continue;
        }
        if let Ok(merge_base) = no_effect::merge_base(root, &candidate, &view.head_ref_oid) {
            if let Ok(base_sha) = no_effect::commit_sha(root, &candidate) {
                return Some((merge_base, base_sha, view.head_ref_oid.clone()));
            }
        }
    }
    None
}

/// Runs `git diff` for the merge-base..head pair.
pub(super) fn local_diff(root: &Path, base: &str, head: &str) -> Result<String> {
    let output = git_command(root)
        .args([
            "diff",
            "--no-color",
            "--find-renames",
            "--unified=3",
            base,
            head,
            "--",
        ])
        .output()
        .context("failed to run git diff")?;
    if !output.status.success() {
        bail!(
            "git diff {base} {head} failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}
