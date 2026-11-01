//! Read-only `GitHub` metadata through the `gh` CLI.

use std::path::Path;

use anyhow::{Context, Result, bail};
use serde::Deserialize;

#[path = "review_threads.rs"]
mod review_threads;
pub use review_threads::review_threads;

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
    crate::gh::json(
        root,
        &[
            "pr",
            "view",
            &number_arg,
            "--json",
            "number,baseRefName,headRefOid",
        ],
        &format!("pr view {number}"),
    )
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
    let output = crate::gh::output(root, &["pr", "diff", &number_arg], "pr diff")?;
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
    let output = crate::gh::output(
        root,
        &["api", &endpoint, "--jq", ".files | length"],
        "api compare",
    )?;
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
    /// ISO-8601 start timestamp, when the payload carries one.
    #[serde(default)]
    pub started_at: Option<String>,
    /// ISO-8601 completion timestamp, when the payload carries one.
    #[serde(default)]
    pub completed_at: Option<String>,
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

#[derive(Deserialize)]
struct CommitStatusesResponse {
    #[serde(default)]
    statuses: Vec<CommitStatus>,
}

#[derive(Deserialize)]
struct PermissionResponse {
    permission: Option<String>,
    #[serde(rename = "role_name")]
    role_name: Option<String>,
}

/// Reads PR view metadata including merge state.
///
/// # Errors
///
/// Returns an error when `gh` fails.
pub fn readiness_view(root: &Path, number: u64) -> Result<PrReadinessView> {
    let number_arg = number.to_string();
    crate::gh::json(
        root,
        &[
            "pr",
            "view",
            &number_arg,
            "--json",
            "number,baseRefName,headRefOid,mergeStateStatus,mergeable,autoMergeRequest",
        ],
        &format!("pr view {number}"),
    )
}

/// Reads all check runs for a commit.
///
/// # Errors
///
/// Returns an error when `gh api` fails.
pub fn check_runs(root: &Path, head: &str) -> Result<Vec<CheckRun>> {
    let endpoint = format!("repos/{{owner}}/{{repo}}/commits/{head}/check-runs?per_page=100");
    // `--jq` streams one object per line so every page is read: a commit can
    // carry more than the 100 runs of the first page.
    let output = crate::gh::output(
        root,
        &["api", "--paginate", "--jq", ".check_runs[]", &endpoint],
        "api check-runs",
    )?;
    crate::gh::json_lines(&output.stdout, "check-runs")
}

/// Reads commit statuses for a commit.
///
/// # Errors
///
/// Returns an error when `gh api` fails.
pub fn commit_statuses(root: &Path, head: &str) -> Result<Vec<CommitStatus>> {
    let endpoint = format!("repos/{{owner}}/{{repo}}/commits/{head}/status?per_page=100");
    let output = crate::gh::output(root, &["api", "--paginate", &endpoint], "api status")?;
    // `--paginate` concatenates one page document per page.
    let pages =
        serde_json::Deserializer::from_slice(&output.stdout).into_iter::<CommitStatusesResponse>();
    let mut statuses = Vec::new();
    for page in pages {
        statuses.extend(page.context("unexpected commit status JSON")?.statuses);
    }
    Ok(statuses)
}

/// Reads top-level issue comments for a PR.
///
/// `--paginate` merges array pages into one array (verified on gh 2.45: 184
/// single-item pages return one 184-element array), so the whole history is
/// read — the API returns oldest-first, and a busy PR has more than one page.
/// The streaming parse also tolerates concatenated page documents.
///
/// # Errors
///
/// Returns an error when `gh api` fails.
pub fn issue_comments(root: &Path, number: u64) -> Result<Vec<Comment>> {
    let endpoint = format!("repos/{{owner}}/{{repo}}/issues/{number}/comments?per_page=100");
    let output = crate::gh::output(
        root,
        &["api", "--paginate", &endpoint],
        "api issue comments",
    )?;
    let pages = serde_json::Deserializer::from_slice(&output.stdout).into_iter::<Vec<Comment>>();
    let mut comments = Vec::new();
    for page in pages {
        comments.extend(page.context("unexpected issue comments JSON")?);
    }
    Ok(comments)
}

/// Reads repository permission for a user via `gh api`.
///
/// # Errors
///
/// Returns an error when `gh api` fails or author permission cannot be determined.
pub fn user_permission(root: &Path, login: &str) -> Result<String> {
    if login.trim().is_empty() {
        bail!("empty login");
    }
    let endpoint = format!("repos/{{owner}}/{{repo}}/collaborators/{login}/permission");
    let res: PermissionResponse =
        crate::gh::json(root, &["api", &endpoint], "api collaborator permission")?;
    let perm = res.role_name.or(res.permission).unwrap_or_default();
    Ok(perm)
}
