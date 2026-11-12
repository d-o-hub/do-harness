//! Serialized shape of a merge-readiness report.

use serde::{Deserialize, Serialize};

use super::CodecovSummary;
use super::authorization::Authorization;
use crate::pr::checks::{CheckBlocker, CheckEntry};

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
    #[serde(default)]
    pub findings: FindingsSummary,
    /// The merge authorizing object; `ready` mirrors it.
    #[serde(default = "default_authorization")]
    pub authorization: Authorization,
    pub blockers: Vec<String>,
    /// Non-fatal policy observations: an empty required set, a rules API error.
    ///
    /// Warnings never change the verdict. An empty required set in particular
    /// is reported rather than silently treated as "nothing to check", because
    /// a ruleset that requires no checks says nothing about whether any ran.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
}

/// A report parsed without an authorization block (older artifacts) must not
/// read as allowed, so the default is `UNKNOWN`.
fn default_authorization() -> Authorization {
    Authorization {
        schema_version: 1,
        decision: super::AuthorizationDecision::Unknown,
        reasons: Vec::new(),
        receipt: None,
    }
}

/// Counts from the findings provenance evaluation (#470).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FindingsSummary {
    /// Total classified findings on the head SHA.
    #[serde(default)]
    pub total: usize,
    /// Findings classified `INTRODUCED`.
    #[serde(default)]
    pub introduced: usize,
    /// Findings classified `REGRESSED`.
    #[serde(default)]
    pub regressed: usize,
    /// Findings classified `PRE_EXISTING`.
    #[serde(default)]
    pub pre_existing: usize,
    /// Findings with no linked issue and no disposition.
    #[serde(default)]
    pub untracked: usize,
    /// Whether the base baseline could be verified.
    #[serde(default)]
    pub base_verified: bool,
    /// Canonical digest over the classified findings.
    #[serde(default)]
    pub digest: String,
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
    /// Folded CI verdict: `PASS`, `FAIL`, `PENDING`, or `UNKNOWN`.
    #[serde(default)]
    pub verdict: String,
    /// Head SHA the verdict is bound to.
    #[serde(rename = "headSha", default)]
    pub head_sha: String,
    /// Checks whose state could not be proven.
    #[serde(default)]
    pub unknown: Vec<String>,
    /// Every classified check and status, with identity and source.
    #[serde(default)]
    pub entries: Vec<CheckEntry>,
    /// Explicit machine-readable blockers.
    #[serde(default)]
    pub blockers: Vec<CheckBlocker>,
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
    #[serde(default)]
    pub review_decision: Option<String>,
    #[serde(default)]
    pub conversation_inventory_digest: String,
    #[serde(default)]
    pub items_count: usize,
    #[serde(default)]
    pub needs_triage: Vec<crate::pr::conversations::NeedsTriageItem>,
    /// The full conversation ledger, carrying each item's classification and
    /// evidence.
    ///
    /// `needs_triage` alone says what still blocks; this says why everything
    /// else did not, which is the auditable half of an `app-report` decision
    /// (#517). It is covered by `conversation_inventory_digest`, so exposing it
    /// does not change the receipt.
    #[serde(default)]
    pub items: Vec<crate::pr::conversations::ConversationItem>,
    /// Submitted reviews currently requesting changes.
    #[serde(default)]
    pub changes_requested_reviews: usize,
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
