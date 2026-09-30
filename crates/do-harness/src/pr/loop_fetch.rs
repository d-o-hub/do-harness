//! `gh` reads for the PR-loop measures (`metrics pr`).
//!
//! Every read targets an explicit `OWNER/NAME`, so the measures describe any
//! repository the caller can read rather than only the current checkout, and
//! every endpoint is fetched with a bounded page count: a metrics snapshot must
//! never turn into an unbounded crawl of a large repository.

use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use super::gh::{CheckRun, Comment};

/// Items requested per page.
const PAGE_SIZE: usize = 100;

/// Pages fetched per endpoint, bounding one measure's API cost.
const MAX_PAGES: usize = 5;

/// One pull request from the list endpoint.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PullRequest {
    /// Pull request number.
    pub number: u64,
    /// `open` | `closed` (merged PRs are closed with `merged_at` set).
    #[serde(default)]
    pub state: String,
    /// Creation timestamp (RFC 3339).
    #[serde(default)]
    pub created_at: String,
    /// Last-update timestamp, used to bound the fetched window.
    #[serde(default)]
    pub updated_at: String,
    /// Merge timestamp, when merged.
    #[serde(default)]
    pub merged_at: Option<String>,
    /// Head reference.
    #[serde(default)]
    pub head: Option<PullHead>,
    /// Whether the PR is a draft.
    #[serde(default)]
    pub draft: bool,
}

impl PullRequest {
    /// Head commit SHA, when the payload carries one.
    #[must_use]
    pub fn head_sha(&self) -> Option<&str> {
        self.head
            .as_ref()
            .map(|head| head.sha.as_str())
            .filter(|sha| !sha.is_empty())
    }
}

/// Head reference of a pull request.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PullHead {
    /// Head commit SHA.
    #[serde(default)]
    pub sha: String,
}

/// One commit on a pull request's head branch.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct Commit {
    /// Commit SHA.
    #[serde(default)]
    pub sha: String,
    /// Commit metadata.
    #[serde(default)]
    pub commit: CommitDetail,
}

/// Commit metadata carrying the timestamps.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct CommitDetail {
    /// Committer stamp (the push time proxy).
    #[serde(default)]
    pub committer: Option<GitStamp>,
    /// Author stamp, used when the committer stamp is absent.
    #[serde(default)]
    pub author: Option<GitStamp>,
}

/// One git timestamp.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GitStamp {
    /// RFC 3339 timestamp.
    #[serde(default)]
    pub date: Option<String>,
}

/// One workflow run for a head commit.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WorkflowRun {
    /// Run id.
    pub id: u64,
    /// Workflow name.
    #[serde(default)]
    pub name: Option<String>,
    /// `queued` | `in_progress` | `completed`.
    #[serde(default)]
    pub status: String,
    /// `success` | `failure` | `cancelled` | …, when completed.
    #[serde(default)]
    pub conclusion: Option<String>,
    /// Attempt counter: 2 means the run was re-run once.
    #[serde(default)]
    pub run_attempt: Option<u32>,
    /// Creation timestamp.
    #[serde(default)]
    pub created_at: String,
    /// Completion timestamp.
    #[serde(default)]
    pub updated_at: String,
}

/// Pull requests updated at or after `since_epoch`, newest first, at most `limit`.
///
/// # Errors
///
/// Returns an error when `gh api` fails or a page does not parse.
pub fn pull_requests(
    root: &Path,
    repo: &str,
    since_epoch: i64,
    limit: usize,
) -> Result<Vec<PullRequest>> {
    let mut prs: Vec<PullRequest> = Vec::new();
    for page in 1..=MAX_PAGES {
        let endpoint = format!(
            "repos/{repo}/pulls?state=all&sort=updated&direction=desc&per_page={PAGE_SIZE}&page={page}"
        );
        let bytes = api(root, &endpoint, None)?;
        let chunk: Vec<PullRequest> =
            serde_json::from_slice(&bytes).context("unexpected pulls JSON")?;
        let short_page = chunk.len() < PAGE_SIZE;
        // A pull request updated between two pages can appear twice; the list
        // is only ever measured once per number.
        for pr in chunk {
            if prs.iter().any(|seen| seen.number == pr.number) {
                continue;
            }
            if let Some(at) = super::super::dora::gh::parse_rfc3339_utc(&pr.updated_at) {
                if at < since_epoch {
                    continue;
                }
            }
            prs.push(pr);
        }
        if prs.len() >= limit {
            prs.truncate(limit);
            break;
        }
        // The list is sorted by `updated_at`: a short page is the last one, and
        // a full page whose oldest entry is already outside the window means
        // every later page is too.
        let reached_window = prs
            .last()
            .and_then(|pr| super::super::dora::gh::parse_rfc3339_utc(&pr.updated_at))
            .is_some_and(|at| at < since_epoch);
        if short_page || reached_window {
            break;
        }
    }
    Ok(prs)
}

/// Commits on a pull request, oldest first.
///
/// # Errors
///
/// Returns an error when `gh api` fails or a page does not parse.
pub fn commits(root: &Path, repo: &str, number: u64) -> Result<Vec<Commit>> {
    let endpoint = format!("repos/{repo}/pulls/{number}/commits?per_page={PAGE_SIZE}");
    let bytes = api(root, &endpoint, Some(".[]"))?;
    stream(&bytes, "commit")
}

/// Workflow runs for a head commit.
///
/// # Errors
///
/// Returns an error when `gh api` fails or a page does not parse.
pub fn runs(root: &Path, repo: &str, head: &str) -> Result<Vec<WorkflowRun>> {
    let endpoint = format!("repos/{repo}/actions/runs?head_sha={head}&per_page={PAGE_SIZE}");
    let bytes = api(root, &endpoint, Some(".workflow_runs[]"))?;
    stream(&bytes, "workflow run")
}

/// Check runs for a head commit, with their timestamps.
///
/// # Errors
///
/// Returns an error when `gh api` fails or a page does not parse.
pub fn check_runs(root: &Path, repo: &str, head: &str) -> Result<Vec<CheckRun>> {
    let endpoint = format!("repos/{repo}/commits/{head}/check-runs?per_page={PAGE_SIZE}");
    let bytes = api(root, &endpoint, Some(".check_runs[]"))?;
    stream(&bytes, "check run")
}

/// Top-level issue comments of a pull request, oldest first.
///
/// # Errors
///
/// Returns an error when `gh api` fails or a page does not parse.
pub fn comments(root: &Path, repo: &str, number: u64) -> Result<Vec<Comment>> {
    let endpoint = format!("repos/{repo}/issues/{number}/comments?per_page={PAGE_SIZE}");
    let bytes = api(root, &endpoint, Some(".[]"))?;
    stream(&bytes, "comment")
}

/// Runs `gh api`, optionally streamed through `jq`, returning stdout.
fn api(root: &Path, endpoint: &str, jq: Option<&str>) -> Result<Vec<u8>> {
    let mut command = Command::new("gh");
    command.current_dir(root).args(["api", "--paginate"]);
    if let Some(filter) = jq {
        command.args(["--jq", filter]);
    }
    let output = command
        .arg(endpoint)
        .output()
        .context("failed to run gh api (is the GitHub CLI installed?)")?;
    if !output.status.success() {
        bail!(
            "gh api {endpoint} failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(output.stdout)
}

/// Parses newline-delimited JSON documents of one kind.
fn stream<T: serde::de::DeserializeOwned>(bytes: &[u8], what: &str) -> Result<Vec<T>> {
    let text = String::from_utf8_lossy(bytes);
    let mut items = Vec::new();
    for line in text.lines().filter(|line| !line.trim().is_empty()) {
        items.push(serde_json::from_str(line).with_context(|| format!("unexpected {what} JSON"))?);
    }
    Ok(items)
}
