//! Read-only `GitHub` metadata through the `gh` CLI.

use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result, bail};
use serde::Deserialize;

/// Minimal PR metadata needed to locate the effective change.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PrView {
    pub number: u64,
    #[serde(rename = "baseRefName")]
    pub base_ref_name: String,
    #[serde(rename = "headRefOid")]
    pub head_ref_oid: String,
}

/// Extended PR metadata for merge-readiness analysis.
#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct PrViewExtended {
    pub number: u64,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub state: String,
    #[serde(default)]
    pub mergeable: bool,
    #[serde(rename = "mergeStateStatus", default)]
    pub merge_state_status: String,
    #[serde(rename = "baseRefName", default)]
    pub base_ref_name: String,
    #[serde(rename = "headRefOid", default)]
    pub head_ref_oid: String,
}

/// User login structure in `GitHub` REST responses.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct UserLogin {
    #[serde(default)]
    pub login: String,
}

/// Check run item from REST API check-runs.
#[derive(Debug, Clone, Deserialize)]
pub struct CheckRunItem {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub status: String,
    pub conclusion: Option<String>,
    pub html_url: Option<String>,
    pub details_url: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CheckRunsResponse {
    #[serde(default)]
    check_runs: Vec<CheckRunItem>,
}

/// Commit status item from REST API status.
#[derive(Debug, Clone, Deserialize)]
pub struct CommitStatusItem {
    #[serde(default)]
    pub context: String,
    #[serde(default)]
    pub state: String,
    pub target_url: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CommitStatusResponse {
    #[serde(default)]
    statuses: Vec<CommitStatusItem>,
}

/// Review comment item from REST API pulls/{n}/comments.
#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct ReviewCommentItem {
    #[serde(default)]
    pub id: u64,
    #[serde(default)]
    pub user: UserLogin,
    #[serde(default)]
    pub body: String,
    #[serde(default)]
    pub path: String,
    pub line: Option<u64>,
    pub html_url: Option<String>,
}

/// PR review item from REST API pulls/{n}/reviews.
#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct PrReviewItem {
    #[serde(default)]
    pub id: u64,
    #[serde(default)]
    pub user: UserLogin,
    #[serde(default)]
    pub state: String,
}

/// Issue comment item from REST API issues/{n}/comments.
#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct IssueCommentItem {
    #[serde(default)]
    pub id: u64,
    #[serde(default)]
    pub user: UserLogin,
    #[serde(default)]
    pub body: String,
    pub html_url: Option<String>,
}

/// `GraphQL` review thread item.
#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct GqlThreadItem {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub is_resolved: bool,
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub line: u64,
    #[serde(default)]
    pub author: String,
}

/// Reads `gh pr view` metadata for one PR.
pub fn view(root: &Path, number: u64) -> Result<PrView> {
    let output = Command::new("gh")
        .current_dir(root)
        .args([
            "pr",
            "view",
            &number.to_string(),
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

/// Reads extended `gh pr view` metadata for merge readiness.
pub fn view_extended(root: &Path, number: u64) -> Result<PrViewExtended> {
    let output = Command::new("gh")
        .current_dir(root)
        .args([
            "pr",
            "view",
            &number.to_string(),
            "--json",
            "number,title,url,state,mergeable,mergeStateStatus,baseRefName,headRefOid",
        ])
        .output()
        .context("failed to run gh")?;
    if !output.status.success() {
        bail!(
            "gh pr view {number} failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    serde_json::from_slice(&output.stdout).context("unexpected gh pr view JSON")
}

/// Fetches check runs for a head commit SHA via `gh api`.
pub fn fetch_check_runs(root: &Path, head_sha: &str) -> Result<Vec<CheckRunItem>> {
    let endpoint = format!("repos/{{owner}}/{{repo}}/commits/{head_sha}/check-runs?per_page=100");
    let output = Command::new("gh")
        .current_dir(root)
        .args(["api", &endpoint])
        .output()
        .context("failed to run gh api check-runs")?;
    if !output.status.success() {
        return Ok(Vec::new());
    }
    let resp: CheckRunsResponse =
        serde_json::from_slice(&output.stdout).unwrap_or(CheckRunsResponse {
            check_runs: Vec::new(),
        });
    Ok(resp.check_runs)
}

/// Fetches commit status context items for a head commit SHA via `gh api`.
pub fn fetch_commit_statuses(root: &Path, head_sha: &str) -> Result<Vec<CommitStatusItem>> {
    let endpoint = format!("repos/{{owner}}/{{repo}}/commits/{head_sha}/status");
    let output = Command::new("gh")
        .current_dir(root)
        .args(["api", &endpoint])
        .output()
        .context("failed to run gh api status")?;
    if !output.status.success() {
        return Ok(Vec::new());
    }
    let resp: CommitStatusResponse =
        serde_json::from_slice(&output.stdout).unwrap_or(CommitStatusResponse {
            statuses: Vec::new(),
        });
    Ok(resp.statuses)
}

/// Fetches PR review comments via `gh api`.
pub fn fetch_review_comments(root: &Path, number: u64) -> Result<Vec<ReviewCommentItem>> {
    let endpoint = format!("repos/{{owner}}/{{repo}}/pulls/{number}/comments?per_page=100");
    let output = Command::new("gh")
        .current_dir(root)
        .args(["api", &endpoint])
        .output()
        .context("failed to run gh api pulls/comments")?;
    if !output.status.success() {
        return Ok(Vec::new());
    }
    Ok(serde_json::from_slice(&output.stdout).unwrap_or_default())
}

/// Fetches PR reviews via `gh api`.
pub fn fetch_reviews(root: &Path, number: u64) -> Result<Vec<PrReviewItem>> {
    let endpoint = format!("repos/{{owner}}/{{repo}}/pulls/{number}/reviews?per_page=100");
    let output = Command::new("gh")
        .current_dir(root)
        .args(["api", &endpoint])
        .output()
        .context("failed to run gh api pulls/reviews")?;
    if !output.status.success() {
        return Ok(Vec::new());
    }
    Ok(serde_json::from_slice(&output.stdout).unwrap_or_default())
}

/// Fetches PR issue comments via `gh api`.
pub fn fetch_issue_comments(root: &Path, number: u64) -> Result<Vec<IssueCommentItem>> {
    let endpoint = format!("repos/{{owner}}/{{repo}}/issues/{number}/comments?per_page=100");
    let output = Command::new("gh")
        .current_dir(root)
        .args(["api", &endpoint])
        .output()
        .context("failed to run gh api issues/comments")?;
    if !output.status.success() {
        return Ok(Vec::new());
    }
    Ok(serde_json::from_slice(&output.stdout).unwrap_or_default())
}

#[derive(Deserialize)]
struct GqlAuthor {
    login: Option<String>,
}
#[derive(Deserialize)]
struct GqlNode {
    author: Option<GqlAuthor>,
}
#[derive(Deserialize)]
struct GqlComments {
    nodes: Vec<GqlNode>,
}
#[derive(Deserialize)]
struct GqlThread {
    id: String,
    #[serde(rename = "isResolved")]
    is_resolved: bool,
    path: Option<String>,
    line: Option<u64>,
    comments: Option<GqlComments>,
}
#[derive(Deserialize)]
struct GqlThreads {
    nodes: Vec<GqlThread>,
}
#[derive(Deserialize)]
struct GqlPR {
    #[serde(rename = "reviewThreads")]
    review_threads: GqlThreads,
}
#[derive(Deserialize)]
struct GqlRepo {
    #[serde(rename = "pullRequest")]
    pull_request: GqlPR,
}
#[derive(Deserialize)]
struct GqlData {
    repository: GqlRepo,
}
#[derive(Deserialize)]
struct GqlResp {
    data: GqlData,
}

/// Fetches PR review threads via `GraphQL`.
#[allow(clippy::unnecessary_wraps)]
pub fn fetch_review_threads(root: &Path, number: u64) -> Result<Vec<GqlThreadItem>> {
    let query = r"
query Threads($owner: String!, $name: String!, $number: Int!) {
  repository(owner: $owner, name: $name) {
    pullRequest(number: $number) {
      reviewThreads(first: 100) {
        nodes { id isResolved path line comments(first: 1) { nodes { author { login } } } }
      }
    }
  }
}
";
    let owner_out = Command::new("gh")
        .current_dir(root)
        .args(["repo", "view", "--json", "owner,name"])
        .output();
    let (owner, repo_name) = match owner_out {
        Ok(out) if out.status.success() => {
            #[derive(Deserialize)]
            struct RepoInfo {
                owner: UserLogin,
                name: String,
            }
            match serde_json::from_slice::<RepoInfo>(&out.stdout) {
                Ok(i) => (i.owner.login, i.name),
                Err(_) => return Ok(Vec::new()),
            }
        }
        _ => return Ok(Vec::new()),
    };

    let output = Command::new("gh")
        .current_dir(root)
        .args([
            "api",
            "graphql",
            "-f",
            &format!("query={query}"),
            "-f",
            &format!("owner={owner}"),
            "-f",
            &format!("name={repo_name}"),
            "-F",
            &format!("number={number}"),
        ])
        .output();

    let output = match output {
        Ok(out) if out.status.success() => out,
        _ => return Ok(Vec::new()),
    };

    let resp: GqlResp = match serde_json::from_slice(&output.stdout) {
        Ok(r) => r,
        Err(_) => return Ok(Vec::new()),
    };

    let mut threads = Vec::new();
    for node in resp.data.repository.pull_request.review_threads.nodes {
        let author = node
            .comments
            .and_then(|c| c.nodes.into_iter().next())
            .and_then(|c| c.author)
            .and_then(|a| a.login)
            .unwrap_or_else(|| "unknown".to_string());
        threads.push(GqlThreadItem {
            id: node.id,
            is_resolved: node.is_resolved,
            path: node.path.unwrap_or_default(),
            line: node.line.unwrap_or(0),
            author,
        });
    }
    Ok(threads)
}

/// Reads the unified diff of a PR through `gh pr diff`.
pub fn diff(root: &Path, number: u64) -> Result<String> {
    let output = Command::new("gh")
        .current_dir(root)
        .args(["pr", "diff", &number.to_string()])
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
