//! Read-only `GitHub` metadata through the `gh` CLI.

use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result, bail};
use serde::Deserialize;

/// Minimal PR metadata needed to locate the effective change.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PrView {
    /// Pull request number.
    pub number: u64,
    /// Base branch name.
    #[serde(rename = "baseRefName")]
    pub base_ref_name: String,
    /// Current head commit SHA.
    #[serde(rename = "headRefOid")]
    pub head_ref_oid: String,
}

/// Reads `gh pr view` metadata for one PR.
///
/// # Errors
///
/// Returns an error when `gh` is missing, unauthenticated, or reports a
/// malformed payload.
pub fn view(root: &Path, number: u64) -> Result<PrView> {
    let number_arg = number.to_string();
    let output = Command::new("gh")
        .current_dir(root)
        .args([
            "pr",
            "view",
            &number_arg,
            "--json",
            "number,baseRefName,headRefOid",
        ])
        .output()
        .context("failed to run gh (is the GitHub CLI installed?)")?;
    if !output.status.success() {
        bail!(
            "gh pr view {number} failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    serde_json::from_slice(&output.stdout).context("unexpected gh pr view JSON")
}

/// Reads the unified diff of a PR through `gh pr diff`.
///
/// Used when the local clone cannot resolve the PR revisions, for example a
/// shallow checkout.
///
/// # Errors
///
/// Returns an error when `gh` cannot produce the diff.
pub fn diff(root: &Path, number: u64) -> Result<String> {
    let number_arg = number.to_string();
    let output = Command::new("gh")
        .current_dir(root)
        .args(["pr", "diff", &number_arg])
        .output()
        .context("failed to run gh (is the GitHub CLI installed?)")?;
    if !output.status.success() {
        bail!(
            "gh pr diff {number} failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Counts changed files for `base...head` through the compare API.
///
/// Used when the local clone cannot resolve the revisions, for example a
/// shallow checkout. An empty file list means no effective change.
///
/// # Errors
///
/// Returns an error when `gh` cannot answer the compare request.
pub fn compare_file_count(root: &Path, base: &str, head: &str) -> Result<usize> {
    let endpoint = format!("repos/{{owner}}/{{repo}}/compare/{base}...{head}");
    let output = Command::new("gh")
        .current_dir(root)
        .args(["api", &endpoint, "--jq", ".files | length"])
        .output()
        .context("failed to run gh api")?;
    if !output.status.success() {
        bail!(
            "gh api compare {base}...{head} failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    String::from_utf8_lossy(&output.stdout)
        .trim()
        .parse::<usize>()
        .context("unexpected compare API output")
}
