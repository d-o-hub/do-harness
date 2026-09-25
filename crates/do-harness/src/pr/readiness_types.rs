//! Data structures for PR merge-readiness analysis.

use serde::{Deserialize, Serialize};

/// Schema version for readiness report.
pub const SCHEMA_VERSION: u32 = 1;

/// Complete merge-readiness report for a PR.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReadinessReport {
    pub schema_version: u32,
    pub pr: u64,
    pub title: String,
    pub url: String,
    pub head_sha: String,
    pub base_branch: String,
    pub merge_state: String,
    pub mergeable: bool,
    pub checks: CheckSummary,
    pub comments: CommentSummary,
    pub codecov: Option<CodecovDigest>,
    pub actionable_items: Vec<ActionableItem>,
    pub ready_to_merge: bool,
}

/// Bucket breakdown for check runs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckSummary {
    pub passed: usize,
    pub cancelled: usize,
    pub failed: usize,
    pub pending: usize,
    pub skipped: usize,
    pub details: Vec<CheckDetail>,
}

/// Individual check run detail.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckDetail {
    pub name: String,
    pub status: String,
    pub conclusion: String,
    pub bucket: String,
    pub url: Option<String>,
    pub rerun_command: Option<String>,
}

/// Inventory of PR comments and review threads.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommentSummary {
    pub human_comments_count: usize,
    pub bot_comments_count: usize,
    pub review_threads_total: usize,
    pub review_threads_unresolved: usize,
}

/// Codecov coverage finding digest.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodecovDigest {
    pub present: bool,
    pub patch_coverage: Option<String>,
    pub status: String,
    pub summary: String,
    pub comment_url: Option<String>,
}

/// Actionable item blocking merge.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionableItem {
    pub kind: String,
    pub title: String,
    pub description: String,
    pub rerun_command: Option<String>,
    pub url: Option<String>,
}
