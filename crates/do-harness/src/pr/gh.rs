//! Read-only `GitHub` metadata through the `gh` CLI.

use std::path::Path;

use anyhow::{Context, Result, bail};
use serde::Deserialize;

#[path = "review_threads.rs"]
mod review_threads;
pub use review_threads::{ReviewThread, review_threads};

#[path = "permission.rs"]
mod permission;
pub use permission::{PermissionOutcome, user_permission, user_permission_outcome};

#[allow(unused_imports)]
#[path = "gh_sweep.rs"]
mod sweep_extras;
#[allow(unused_imports)]
pub use sweep_extras::{
    MergeCommitRef, MergedPrView, PrListEntry, close_with_comment, disable_auto_merge,
    list_open_prs, merge, merged_view, pr_entry_view, pr_file_set, update_branch,
};

/// Minimal PR metadata needed to locate the effective change.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PrView {
    /// Pull request number.
    pub number: u64,
    /// Base branch name.
    #[serde(rename = "baseRefName")]
    pub base_ref_name: String,
    /// Current base branch commit SHA, when `GitHub` reports one.
    #[serde(rename = "baseRefOid", default)]
    pub base_ref_oid: String,
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
            "number,baseRefName,baseRefOid,headRefOid",
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
    #[serde(rename = "baseRefOid", default)]
    pub base_ref_oid: String,
    #[serde(rename = "headRefName", default)]
    pub head_ref_name: String,
    #[serde(rename = "headRefOid", default)]
    pub head_ref_oid: String,
    #[serde(rename = "mergeStateStatus", default)]
    pub merge_state_status: String,
    #[serde(default)]
    pub mergeable: String,
    #[serde(rename = "autoMergeRequest")]
    pub auto_merge_request: Option<serde_json::Value>,
    #[serde(rename = "reviewDecision", default)]
    pub review_decision: Option<String>,
    /// `OPEN`, `CLOSED` or `MERGED`.
    #[serde(default)]
    pub state: String,
    #[serde(rename = "isDraft", default)]
    pub is_draft: bool,
    #[serde(default)]
    pub labels: Vec<PrLabel>,
    #[serde(rename = "headRepository", default)]
    pub head_repository: Option<PrRepositoryRef>,
}

/// One label on a pull request.
#[derive(Debug, Clone, Deserialize)]
pub struct PrLabel {
    #[serde(default)]
    pub name: String,
}

/// Repository identity for a PR head.
#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct PrRepositoryRef {
    #[serde(rename = "nameWithOwner", default)]
    pub name_with_owner: String,
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
    /// The app that created the check run, when `GitHub` reports one.
    #[serde(default)]
    pub app: Option<CheckApp>,
    /// ISO-8601 start timestamp, when the payload carries one.
    #[serde(default)]
    pub started_at: Option<String>,
    /// ISO-8601 completion timestamp, when the payload carries one.
    #[serde(default)]
    pub completed_at: Option<String>,
}

/// The app that owns a check run.
#[derive(Debug, Clone, Deserialize)]
pub struct CheckApp {
    /// App slug, e.g. `github-actions` or `dependabot`.
    #[serde(default)]
    pub slug: Option<String>,
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

/// One page of the check-runs API, including the total the API claims.
#[derive(Debug, Clone, Deserialize)]
pub struct CheckRunsPage {
    #[serde(default)]
    pub total_count: u64,
    #[serde(default)]
    pub check_runs: Vec<CheckRun>,
}

/// One page of the commit-status API, including the total the API claims.
#[derive(Debug, Clone, Deserialize)]
pub struct StatusPage {
    #[serde(default)]
    pub total_count: u64,
    #[serde(default)]
    pub statuses: Vec<CommitStatus>,
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

/// Author of a `GitHub` comment, review, or pull request.
#[derive(Debug, Clone, Deserialize)]
pub struct CommentAuthor {
    pub login: String,
    /// `GitHub`'s own App/bot classification, as `gh pr list --json author`
    /// reports it. Defaults to `false` for payloads that omit the field (`gh
    /// api` comments and reviews do), so the name predicate remains the
    /// fallback rather than the field's absence silently demoting a real bot.
    #[serde(default)]
    pub is_bot: bool,
}

#[derive(Deserialize)]
struct HeadView {
    #[serde(rename = "headRefOid")]
    head_ref_oid: String,
}

/// Reads the current head SHA of a PR.
///
/// # Errors
///
/// Returns an error when `gh` fails or reports an empty head SHA.
pub fn head_sha(root: &Path, number: u64) -> Result<String> {
    let number_arg = number.to_string();
    let view: HeadView = crate::gh::json(
        root,
        &["pr", "view", &number_arg, "--json", "headRefOid"],
        &format!("pr view {number}"),
    )?;
    if view.head_ref_oid.trim().is_empty() {
        bail!("PR {number} reported an empty head SHA");
    }
    Ok(view.head_ref_oid)
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
            "number,baseRefName,baseRefOid,headRefName,headRefOid,mergeStateStatus,mergeable,autoMergeRequest,reviewDecision,state,isDraft,labels,headRepository",
        ],
        &format!("pr view {number}"),
    )
}

/// Reads every check-run page for a commit and the API's claimed total.
///
/// `--paginate --slurp` returns one array of page documents, so the caller can
/// compare the collected runs with `total_count` instead of trusting a
/// truncated first page.
///
/// # Errors
///
/// Returns an error when `gh api` fails or the payload is malformed.
pub fn check_runs_page(root: &Path, head: &str) -> Result<CheckRunsPage> {
    let endpoint = format!("repos/{{owner}}/{{repo}}/commits/{head}/check-runs?per_page=100");
    let output = crate::gh::output(
        root,
        &["api", "--paginate", "--slurp", &endpoint],
        "api check-runs",
    )?;
    let pages: Vec<CheckRunsPage> =
        serde_json::from_slice(&output.stdout).context("unexpected check-runs JSON")?;
    let total_count = pages.first().map_or(0, |page| page.total_count);
    let mut check_runs = Vec::new();
    for page in pages {
        check_runs.extend(page.check_runs);
    }
    Ok(CheckRunsPage {
        total_count,
        check_runs,
    })
}

/// Reads every commit-status page for a commit and the API's claimed total.
///
/// # Errors
///
/// Returns an error when `gh api` fails or the payload is malformed.
pub fn commit_statuses_page(root: &Path, head: &str) -> Result<StatusPage> {
    let endpoint = format!("repos/{{owner}}/{{repo}}/commits/{head}/status?per_page=100");
    let output = crate::gh::output(
        root,
        &["api", "--paginate", "--slurp", &endpoint],
        "api status",
    )?;
    let pages: Vec<StatusPage> =
        serde_json::from_slice(&output.stdout).context("unexpected commit status JSON")?;
    let total_count = pages.first().map_or(0, |page| page.total_count);
    let mut statuses = Vec::new();
    for page in pages {
        statuses.extend(page.statuses);
    }
    Ok(StatusPage {
        total_count,
        statuses,
    })
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

/// A submitted review on a pull request.
#[derive(Debug, Clone, Deserialize)]
pub struct Review {
    pub id: u64,
    #[serde(rename = "node_id", default)]
    pub node_id: Option<String>,
    #[serde(default)]
    pub user: Option<CommentAuthor>,
    #[serde(default)]
    pub body: String,
    #[serde(default)]
    pub state: String,
    pub submitted_at: Option<String>,
    pub html_url: Option<String>,
    pub commit_id: Option<String>,
}

/// Reads all submitted reviews on a pull request.
///
/// # Errors
///
/// Returns an error when `gh api` fails or the response cannot be parsed.
pub fn reviews(root: &Path, number: u64) -> Result<Vec<Review>> {
    let endpoint = format!("repos/{{owner}}/{{repo}}/pulls/{number}/reviews?per_page=100");
    let output = crate::gh::output(
        root,
        &["api", "--paginate", &endpoint],
        "api pull request reviews",
    )?;
    let pages = serde_json::Deserializer::from_slice(&output.stdout).into_iter::<Vec<Review>>();
    let mut reviews = Vec::new();
    for page in pages {
        reviews.extend(page.context("unexpected reviews JSON")?);
    }
    Ok(reviews)
}
