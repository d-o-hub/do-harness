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

/// Metadata needed for evaluating PR merge readiness.
#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct PrReadinessView {
    pub number: u64,
    #[serde(rename = "baseRefName", default)]
    pub base_ref_name: String,
    #[serde(rename = "headRefOid", default)]
    pub head_ref_oid: String,
    #[serde(rename = "mergeStateStatus", default)]
    pub merge_state_status: String,
    #[serde(default)]
    pub mergeable: String,
    #[serde(rename = "autoMergeRequest")]
    pub auto_merge_request: Option<serde_json::Value>,
}

/// A check run returned by `GitHub`'s check-runs API.
#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct CheckRun {
    pub id: Option<u64>,
    pub name: String,
    #[serde(default)]
    pub status: String,
    pub conclusion: Option<String>,
    pub html_url: Option<String>,
}

impl CheckRun {
    /// Derives the rerun command for a failed or cancelled check run.
    #[must_use]
    pub fn rerun_command(&self) -> String {
        if let Some(url) = &self.html_url {
            if let Some(run_idx) = url.find("/actions/runs/") {
                let rest = &url[run_idx + "/actions/runs/".len()..];
                let run_id = rest.split('/').next().unwrap_or("");
                if let Some(job_idx) = rest.find("/job/") {
                    let job_id = rest[job_idx + "/job/".len()..]
                        .split(['/', '?'])
                        .next()
                        .unwrap_or("");
                    if !run_id.is_empty() && !job_id.is_empty() {
                        return format!("gh run rerun {run_id} --job {job_id}");
                    }
                }
                if !run_id.is_empty() {
                    return format!("gh run rerun {run_id}");
                }
            }
        }
        format!("gh run rerun <run-id> # for check '{}'", self.name)
    }
}

/// A commit status returned by `GitHub`'s status API.
#[derive(Debug, Clone, Deserialize)]
pub struct CommitStatus {
    #[serde(default)]
    pub context: String,
    #[serde(default)]
    pub state: String,
    pub target_url: Option<String>,
}

/// A comment on an issue or PR.
#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct Comment {
    pub id: Option<u64>,
    #[serde(default)]
    pub user: Option<CommentAuthor>,
    #[serde(default)]
    pub body: String,
    #[serde(default)]
    pub created_at: String,
    pub html_url: Option<String>,
}

/// Author of a `GitHub` comment.
#[derive(Debug, Clone, Deserialize)]
pub struct CommentAuthor {
    pub login: String,
}

/// A review thread returned by `GraphQL` query.
#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct ReviewThread {
    pub id: String,
    #[serde(rename = "isResolved", default)]
    pub is_resolved: bool,
    #[serde(rename = "isOutdated", default)]
    pub is_outdated: bool,
    pub path: Option<String>,
    pub line: Option<u64>,
    #[serde(default)]
    pub comments: ReviewThreadComments,
}

/// Wrapper for comments in a review thread.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct ReviewThreadComments {
    #[serde(default)]
    pub nodes: Vec<ReviewThreadComment>,
}

/// A single comment in a review thread.
#[derive(Debug, Clone, Deserialize)]
pub struct ReviewThreadComment {
    pub author: Option<CommentAuthor>,
    #[serde(default)]
    pub body: String,
}
#[derive(Deserialize)]
struct CheckRunsResponse {
    #[serde(default)]
    check_runs: Vec<CheckRun>,
}

#[derive(Deserialize)]
struct CommitStatusesResponse {
    #[serde(default)]
    statuses: Vec<CommitStatus>,
}

#[derive(Deserialize)]
struct GqlResponse {
    data: Option<GqlData>,
}

#[derive(Deserialize)]
struct GqlData {
    repository: Option<GqlRepo>,
}

#[derive(Deserialize)]
struct GqlRepo {
    #[serde(rename = "pullRequest")]
    pull_request: Option<GqlPr>,
}

#[derive(Deserialize)]
struct GqlPr {
    #[serde(rename = "reviewThreads")]
    review_threads: Option<GqlThreads>,
}

#[derive(Deserialize)]
struct GqlThreads {
    #[serde(default)]
    nodes: Vec<ReviewThread>,
}

/// Reads PR view metadata including merge state.
///
/// # Errors
///
/// Returns an error when `gh` fails.
pub fn readiness_view(root: &Path, number: u64) -> Result<PrReadinessView> {
    let number_arg = number.to_string();
    let output = Command::new("gh")
        .current_dir(root)
        .args([
            "pr",
            "view",
            &number_arg,
            "--json",
            "number,baseRefName,headRefOid,mergeStateStatus,mergeable,autoMergeRequest",
        ])
        .output()
        .context("failed to run gh pr view")?;
    if !output.status.success() {
        bail!(
            "gh pr view {number} failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    serde_json::from_slice(&output.stdout).context("unexpected gh pr view JSON")
}

/// Reads all check runs for a commit.
///
/// # Errors
///
/// Returns an error when `gh api` fails.
pub fn check_runs(root: &Path, head: &str) -> Result<Vec<CheckRun>> {
    let endpoint = format!("repos/{{owner}}/{{repo}}/commits/{head}/check-runs?per_page=100");
    let output = Command::new("gh")
        .current_dir(root)
        .args(["api", &endpoint])
        .output()
        .context("failed to run gh api check-runs")?;
    if !output.status.success() {
        bail!(
            "gh api check-runs failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    let wrapper: CheckRunsResponse =
        serde_json::from_slice(&output.stdout).context("unexpected check-runs JSON")?;
    Ok(wrapper.check_runs)
}

/// Reads commit statuses for a commit.
///
/// # Errors
///
/// Returns an error when `gh api` fails.
pub fn commit_statuses(root: &Path, head: &str) -> Result<Vec<CommitStatus>> {
    let endpoint = format!("repos/{{owner}}/{{repo}}/commits/{head}/status");
    let output = Command::new("gh")
        .current_dir(root)
        .args(["api", &endpoint])
        .output()
        .context("failed to run gh api status")?;
    if !output.status.success() {
        bail!(
            "gh api status failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    let wrapper: CommitStatusesResponse =
        serde_json::from_slice(&output.stdout).context("unexpected commit status JSON")?;
    Ok(wrapper.statuses)
}

/// Reads top-level issue comments for a PR.
///
/// # Errors
///
/// Returns an error when `gh api` fails.
pub fn issue_comments(root: &Path, number: u64) -> Result<Vec<Comment>> {
    let endpoint = format!("repos/{{owner}}/{{repo}}/issues/{number}/comments?per_page=100");
    let output = Command::new("gh")
        .current_dir(root)
        .args(["api", &endpoint])
        .output()
        .context("failed to run gh api issue comments")?;
    if !output.status.success() {
        bail!(
            "gh api issue comments failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    serde_json::from_slice(&output.stdout).context("unexpected issue comments JSON")
}

/// Reads review threads for a PR via `GraphQL`.
///
/// # Errors
///
/// Returns an error when `gh api` fails.
pub fn review_threads(root: &Path, number: u64) -> Result<Vec<ReviewThread>> {
    let query = r#"query($number: Int!) {
        repository(owner: "{owner}", name: "{repo}") {
            pullRequest(number: $number) {
                reviewThreads(first: 100) {
                    nodes {
                        id
                        isResolved
                        isOutdated
                        path
                        line
                        comments(first: 1) {
                            nodes {
                                author { login }
                                body
                            }
                        }
                    }
                }
            }
        }
    }"#;
    let output = Command::new("gh")
        .current_dir(root)
        .args([
            "api",
            "graphql",
            "-F",
            &format!("number={number}"),
            "-f",
            &format!("query={query}"),
        ])
        .output()
        .context("failed to run gh api graphql reviewThreads")?;
    if !output.status.success() {
        return Ok(Vec::new());
    }
    let res: GqlResponse =
        serde_json::from_slice(&output.stdout).unwrap_or(GqlResponse { data: None });
    Ok(res
        .data
        .and_then(|d| d.repository)
        .and_then(|r| r.pull_request)
        .and_then(|p| p.review_threads)
        .map(|t| t.nodes)
        .unwrap_or_default())
}
