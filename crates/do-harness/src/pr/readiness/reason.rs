//! Authorization verdicts, reason codes, and the receipt shape.

use serde::{Deserialize, Serialize};

/// Receipt schema version.
pub const RECEIPT_SCHEMA_VERSION: u32 = 1;
/// Authorizing receipt lifetime in seconds.
pub const RECEIPT_LIFETIME_SECONDS: i64 = 60;
/// Labels that make a pull request a hard stop.
pub const HELD_LABELS: [&str; 3] = ["hold", "wip", "do-not-merge"];

pub mod codes {
    /// The PR is a draft.
    pub const DRAFT: &str = "draft";
    /// The PR carries a hold/wip/do-not-merge label.
    pub const HELD: &str = "held";
    /// The PR is not open.
    pub const NOT_OPEN: &str = "state_not_open";
    /// A required check failed.
    pub const CHECKS_FAILED: &str = "checks_failed";
    /// A required check is still running.
    pub const CHECKS_PENDING: &str = "checks_pending";
    /// A check's state could not be classified.
    pub const CHECKS_UNKNOWN: &str = "checks_unknown";
    /// Reviews request changes.
    pub const REVIEW_CHANGES_REQUESTED: &str = "review_changes_requested";
    /// A review thread is unresolved, including outdated ones.
    pub const CONVERSATION_UNRESOLVED: &str = "conversation_unresolved";
    /// A conversation item still needs triage.
    pub const CONVERSATION_NEEDS_TRIAGE: &str = "conversation_needs_triage";
    /// The declared merge policy drifted from the server ruleset.
    pub const POLICY_DRIFT: &str = "policy_drift";
    /// The merge policy could not be evaluated.
    pub const POLICY_UNAVAILABLE: &str = "policy_unavailable";
    /// An introduced or regressed finding is untracked.
    pub const FINDINGS_UNTRACKED: &str = "findings_untracked";
    /// The base baseline could not be verified.
    pub const FINDINGS_UNVERIFIED_BASELINE: &str = "findings_unverified_baseline";
    /// `GitHub` did not report every field the snapshot needs.
    pub const API_INCOMPLETE: &str = "api_incomplete";
    /// The snapshot changed while it was being collected.
    pub const SNAPSHOT_MOVED: &str = "snapshot_moved";
    /// The head does not contain the base commit.
    pub const ANCESTRY_DIVERGED: &str = "ancestry_diverged";
    /// Ancestry could not be proven.
    pub const ANCESTRY_UNKNOWN: &str = "ancestry_unknown";
    /// The merge state is not clean or the head is behind its base.
    pub const MERGE_STATE_DIRTY: &str = "merge_state_dirty";
    /// An auto-merge request is armed.
    pub const AUTO_MERGE_ARMED: &str = "auto_merge_armed";
    /// The supplied receipt expired.
    pub const RECEIPT_EXPIRED: &str = "receipt_expired";
    /// The supplied receipt no longer matches the recomputed snapshot.
    pub const RECEIPT_CHANGED: &str = "receipt_changed";
    /// The supplied receipt is malformed, unsigned by this tool, or for another PR.
    pub const RECEIPT_INVALID: &str = "receipt_invalid";
}

/// Authorization verdict for a direct merge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AuthorizationDecision {
    /// Every input is present, known, and permits a merge.
    MergeAllowed,
    /// A known condition blocks the merge.
    Blocked,
    /// The inputs needed to decide could not be collected.
    Unknown,
}

impl AuthorizationDecision {
    /// Stable uppercase label.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::MergeAllowed => "MERGE_ALLOWED",
            Self::Blocked => "BLOCKED",
            Self::Unknown => "UNKNOWN",
        }
    }
}

/// One reason an authorization did not succeed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthorizationReason {
    /// Stable code from [`codes`].
    pub code: String,
    /// Human-readable detail.
    pub message: String,
    /// Related URL, when the source reported one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

impl AuthorizationReason {
    pub(crate) fn new(code: &str, message: impl Into<String>) -> Self {
        Self {
            code: code.to_owned(),
            message: message.into(),
            url: None,
        }
    }
}

/// The exact PR state a receipt authorizes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MergeReceipt {
    /// Receipt schema version.
    pub schema_version: u32,
    /// Pull request number.
    pub pr: u64,
    /// UTC epoch seconds when the snapshot was sealed.
    pub issued_at_unix: i64,
    /// UTC epoch seconds after which the receipt is void.
    pub expires_at_unix: i64,
    /// Canonical digest of every authorization input.
    pub snapshot_sha256: String,
    /// Base branch name.
    pub base_ref_name: String,
    /// Captured base commit SHA.
    pub base_ref_oid: String,
    /// Head branch name.
    pub head_ref_name: String,
    /// Captured head commit SHA.
    pub head_ref_oid: String,
    /// `owner/name` of the repository the head branch lives in, when reported.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub head_repository: Option<String>,
    /// PR state (`OPEN` only authorizes).
    pub state: String,
    /// Draft flag.
    pub is_draft: bool,
    /// Sorted label names.
    pub labels: Vec<String>,
    /// `mergeable` as reported.
    pub mergeable: String,
    /// `mergeStateStatus` as reported.
    pub merge_state_status: String,
    /// Whether an auto-merge request is armed.
    pub auto_merge: bool,
    /// `reviewDecision`, or `None` when `GitHub` reports none.
    pub review_decision: Option<String>,
    /// Digest of the classified check set.
    pub checks_sha256: String,
    /// Digest of the conversation ledger.
    pub conversations_sha256: String,
    /// Digest of the classified findings.
    pub findings_sha256: String,
    /// Merge policy revision digest, or `absent`.
    ///
    /// This is the overlay's `sha256` when one decided, and the server-derived
    /// rules' `rules_sha256` when they did. Either way a change between
    /// collection and merge invalidates the receipt exactly like a moved head.
    pub policy_revision: String,
    /// Which source produced the policy decision (`overlay` or `derived`).
    #[serde(default)]
    pub policy_source: String,
    /// Ancestry verdict.
    pub ancestry: String,
    /// Base commit the compare API confirmed.
    pub ancestry_base_commit: String,
    /// Merge base the compare API confirmed.
    pub ancestry_merge_base: String,
}

/// The authorization object embedded in a readiness report.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Authorization {
    /// Schema version.
    pub schema_version: u32,
    /// Verdict.
    pub decision: AuthorizationDecision,
    /// Every blocking or unknown reason, in collection order.
    pub reasons: Vec<AuthorizationReason>,
    /// Present only for `MERGE_ALLOWED`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub receipt: Option<MergeReceipt>,
}
