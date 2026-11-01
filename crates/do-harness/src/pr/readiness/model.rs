//! Serialized shape of a merge-readiness report.

use serde::{Deserialize, Serialize};

use super::CodecovSummary;

fn default_true() -> bool {
    true
}

/// Merge readiness evaluation report.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReadinessReport {
    pub pr: u64,
    #[serde(rename = "headRefOid", default)]
    pub head_ref_oid: String,
    #[serde(rename = "collectionComplete", default = "default_true")]
    pub collection_complete: bool,
    pub ready: bool,
    pub merge_state: MergeState,
    pub checks: CheckSummary,
    pub conversations: ConversationSummary,
    pub codecov: Option<CodecovSummary>,
    pub blockers: Vec<String>,
}

/// PR mergeability state from `GitHub`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MergeState {
    pub status: String,
    pub mergeable: String,
    pub auto_merge: bool,
}

/// Categorized CI checks and commit statuses.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckSummary {
    pub total: usize,
    pub passed: usize,
    pub failed: Vec<FailedCheck>,
    pub cancelled: Vec<CancelledCheck>,
    pub pending: Vec<String>,
    pub skipped: usize,
    /// Whether these are all the checks the merge is meant to wait for.
    ///
    /// A merge conflict stops `GitHub` from scheduling workflow checks at all,
    /// so a conflicting pull request reports only the checks that do not need a
    /// mergeable head. Counting those as a passing suite would read as green
    /// while the verification never ran.
    #[serde(default)]
    pub incomplete: bool,
}

/// A failed check run or commit status.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailedCheck {
    pub name: String,
    pub url: Option<String>,
}

/// A cancelled check run with exact rerun command.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CancelledCheck {
    pub name: String,
    pub rerun_command: String,
    pub url: Option<String>,
}

/// Review threads and actionable comments.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationSummary {
    pub unresolved_threads: Vec<UnresolvedThread>,
    pub actionable_comments: Vec<ActionableComment>,
    #[serde(default = "default_true")]
    pub complete: bool,
}

/// An unresolved review thread.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnresolvedThread {
    pub id: String,
    pub path: Option<String>,
    pub line: Option<u64>,
    pub author: String,
    pub excerpt: String,
    #[serde(rename = "isOutdated", default)]
    pub is_outdated: bool,
}

/// An actionable top-level comment (e.g. Codecov gap).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionableComment {
    pub id: Option<u64>,
    pub author: String,
    pub excerpt: String,
    pub url: Option<String>,
}
