//! Sweep transport through `gh`: inventory, files, update-branch, disarm,
//! merge, and close. Kept out of `gh.rs` for the LOC ceiling.

use std::path::Path;

use anyhow::{Context, Result};
use serde::Deserialize;

use super::{CommentAuthor, PrLabel, PrRepositoryRef};

/// One open pull request as reported by `gh pr list` (or `gh pr view`).
#[derive(Debug, Clone, Deserialize)]
pub struct PrListEntry {
    /// Pull request number.
    pub number: u64,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub author: Option<CommentAuthor>,
    #[serde(rename = "createdAt", default)]
    pub created_at: String,
    #[serde(rename = "updatedAt", default)]
    pub updated_at: String,
    #[serde(rename = "isDraft", default)]
    pub is_draft: bool,
    /// `OPEN`, `CLOSED`, or `MERGED`.
    #[serde(default)]
    pub state: String,
    #[serde(rename = "baseRefName", default)]
    pub base_ref_name: String,
    #[serde(rename = "baseRefOid", default)]
    pub base_ref_oid: String,
    #[serde(rename = "headRefName", default)]
    pub head_ref_name: String,
    #[serde(rename = "headRefOid", default)]
    pub head_ref_oid: String,
    #[serde(rename = "mergeStateStatus", default)]
    pub merge_state_status: String,
    #[serde(default)]
    pub labels: Vec<PrLabel>,
    #[allow(dead_code)]
    #[serde(rename = "headRepository", default)]
    pub head_repository: Option<PrRepositoryRef>,
}

const LIST_JSON_FIELDS: &str = "number,title,url,author,createdAt,updatedAt,isDraft,state,baseRefName,baseRefOid,headRefName,headRefOid,mergeStateStatus,labels,headRepository";

/// Reads every open pull request (bounded at 200).
///
/// # Errors
///
/// Returns an error when `gh` fails or the payload is malformed.
pub fn list_open_prs(root: &Path) -> Result<Vec<PrListEntry>> {
    crate::gh::json(
        root,
        &[
            "pr",
            "list",
            "--state",
            "open",
            "--limit",
            "200",
            "--json",
            LIST_JSON_FIELDS,
        ],
        "pr list",
    )
}

/// Reads the list-shaped view of one pull request.
///
/// Single-target sweeps (`--pr N`) need the same author/created/label fields
/// as the open list even when the PR is closed or merged.
pub fn pr_entry_view(root: &Path, number: u64) -> Result<PrListEntry> {
    let number_arg = number.to_string();
    crate::gh::json(
        root,
        &["pr", "view", &number_arg, "--json", LIST_JSON_FIELDS],
        &format!("pr view {number}"),
    )
}

/// The merge commit of a merged pull request.
#[derive(Debug, Clone, Deserialize)]
pub struct MergeCommitRef {
    #[serde(default)]
    pub oid: String,
}

/// Post-merge PR view: state plus merge commit.
#[derive(Debug, Clone, Deserialize)]
pub struct MergedPrView {
    #[serde(default)]
    pub state: String,
    #[serde(rename = "mergeCommit", default)]
    pub merge_commit: Option<MergeCommitRef>,
}

/// Reads `state` and `mergeCommit` for a PR.
///
/// # Errors
///
/// Returns an error when `gh` fails.
pub fn merged_view(root: &Path, number: u64) -> Result<MergedPrView> {
    let number_arg = number.to_string();
    crate::gh::json(
        root,
        &["pr", "view", &number_arg, "--json", "state,mergeCommit"],
        &format!("pr view {number}"),
    )
}

/// Reads every changed file path of a pull request.
///
/// # Errors
///
/// Returns an error when `gh api` fails.
pub fn pr_file_set(root: &Path, number: u64) -> Result<Vec<String>> {
    let endpoint = format!("repos/{{owner}}/{{repo}}/pulls/{number}/files?per_page=100");
    let output = crate::gh::output(root, &["api", "--paginate", &endpoint], "api pull files")?;
    let pages = serde_json::Deserializer::from_slice(&output.stdout).into_iter::<Vec<PullFile>>();
    let mut files = Vec::new();
    for page in pages {
        for file in page.context("unexpected pull files JSON")? {
            files.push(file.filename);
        }
    }
    let mut seen = std::collections::BTreeSet::new();
    files.retain(|name| seen.insert(name.clone()));
    Ok(files)
}

#[derive(Deserialize)]
struct PullFile {
    #[serde(default)]
    filename: String,
}

/// Submits an update-branch request pinned to the captured head.
///
/// The API refuses (`422`) when the head moved since it was captured, so the
/// call cannot overwrite newer author work.
pub fn update_branch(root: &Path, number: u64, expected_head_sha: &str) -> Result<()> {
    let endpoint = format!("repos/{{owner}}/{{repo}}/pulls/{number}/update-branch");
    let expected = format!("expected_head_sha={expected_head_sha}");
    crate::gh::output(
        root,
        &["api", "--method", "PUT", &endpoint, "-f", &expected],
        "api update-branch",
    )?;
    Ok(())
}

/// Disarms a pre-existing auto-merge request.
pub fn disable_auto_merge(root: &Path, number: u64) -> Result<()> {
    let endpoint = format!("repos/{{owner}}/{{repo}}/pulls/{number}/auto-merge");
    crate::gh::output(
        root,
        &["api", "--method", "DELETE", &endpoint],
        "api disable auto-merge",
    )?;
    Ok(())
}

/// Merges a PR with the direct, head-pinned command.
pub fn merge(root: &Path, number: u64, head_sha: &str) -> Result<()> {
    let number_arg = number.to_string();
    crate::gh::output(
        root,
        &[
            "pr",
            "merge",
            &number_arg,
            "--squash",
            "--match-head-commit",
            head_sha,
        ],
        "pr merge",
    )?;
    Ok(())
}

/// Closes a PR with a comment.
pub fn close_with_comment(root: &Path, number: u64, comment: &str) -> Result<()> {
    let number_arg = number.to_string();
    crate::gh::output(
        root,
        &["pr", "close", &number_arg, "--comment", comment],
        "pr close",
    )?;
    Ok(())
}
