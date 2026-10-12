//! Review-thread reads for `pr ready`, with cursor pagination.
//!
//! Split out of `gh.rs` (move-only) to keep that module under the LOC ceiling.

use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result, bail};
use serde::Deserialize;

use super::CommentAuthor;

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
struct GqlResponse {
    data: Option<GqlData>,
    #[serde(default)]
    errors: Vec<GqlError>,
}

#[derive(Deserialize)]
struct GqlError {
    message: String,
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
    #[serde(rename = "pageInfo")]
    page_info: Option<GqlPageInfo>,
}

#[derive(Deserialize, Debug, Clone)]
struct GqlPageInfo {
    #[serde(rename = "hasNextPage", default)]
    has_next_page: bool,
    #[serde(rename = "endCursor")]
    end_cursor: Option<String>,
}

/// Reads review threads for a PR via `GraphQL`.
///
/// # Errors
///
/// Returns an error when `gh api` fails.
pub fn review_threads(root: &Path, number: u64) -> Result<Vec<ReviewThread>> {
    const MAX_PAGES: usize = 100;
    let query = r#"query($number: Int!, $after: String) {
        repository(owner: "{owner}", name: "{repo}") {
            pullRequest(number: $number) {
                reviewThreads(first: 100, after: $after) {
                    pageInfo { hasNextPage endCursor }
                    nodes {
                        id isResolved isOutdated path line
                        comments(first: 1) { nodes { author { login } body } }
                    }
                }
            }
        }
    }"#;

    let mut all_threads = Vec::new();
    let mut seen_ids = std::collections::HashSet::new();
    let mut cursor: Option<String> = None;
    let mut page_count = 0;

    loop {
        page_count += 1;
        if page_count > MAX_PAGES {
            bail!("exceeded maximum review thread pagination limit ({MAX_PAGES} pages)");
        }

        let number_arg = format!("number={number}");
        let query_arg = format!("query={query}");
        let mut args = vec!["api", "graphql", "-F", &number_arg];
        let after_arg;
        if let Some(ref c) = cursor {
            after_arg = format!("after={c}");
            args.push("-F");
            args.push(&after_arg);
        }
        args.push("-F");
        args.push(&query_arg);

        let output = Command::new("gh")
            .current_dir(root)
            .args(&args)
            .output()
            .context("failed to run gh api graphql reviewThreads")?;

        if !output.status.success() {
            bail!(
                "gh api graphql reviewThreads failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            );
        }

        let res: GqlResponse =
            serde_json::from_slice(&output.stdout).context("unexpected reviewThreads JSON")?;

        if let Some(error) = res.errors.first() {
            bail!(
                "gh api graphql reviewThreads returned an error: {}",
                error.message
            );
        }

        let threads_data = res
            .data
            .and_then(|d| d.repository)
            .and_then(|r| r.pull_request)
            .and_then(|p| p.review_threads)
            .ok_or_else(|| anyhow::anyhow!("gh api graphql reviewThreads returned no data"))?;

        for thread in threads_data.nodes {
            if seen_ids.insert(thread.id.clone()) {
                all_threads.push(thread);
            }
        }

        let page_info = threads_data.page_info.unwrap_or(GqlPageInfo {
            has_next_page: false,
            end_cursor: None,
        });

        if !page_info.has_next_page {
            break;
        }

        match page_info.end_cursor {
            Some(next_cursor) if !next_cursor.trim().is_empty() => {
                if cursor.as_deref() == Some(next_cursor.as_str()) {
                    bail!("hasNextPage is true but cursor did not advance ({next_cursor})");
                }
                cursor = Some(next_cursor);
            }
            _ => bail!("hasNextPage is true but no usable advancing cursor was provided"),
        }
    }

    Ok(all_threads)
}
