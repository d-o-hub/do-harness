//! Data types and schemas for PR conversation ledger and dispositions.

use serde::{Deserialize, Serialize};

/// Decision recorded in a structured conversation disposition.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, clap::ValueEnum)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DispositionDecision {
    #[value(name = "fixed")]
    Fixed,
    #[value(name = "non-actionable")]
    NonActionable,
    #[value(name = "waived")]
    Waived,
}

impl DispositionDecision {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Fixed => "FIXED",
            Self::NonActionable => "NON_ACTIONABLE",
            Self::Waived => "WAIVED",
        }
    }
}

/// Durable disposition record formatted inside `<!-- do-harness-disposition:v1 ... -->`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DispositionRecord {
    pub schema_version: u32,
    pub item_ref: String,
    pub decision: DispositionDecision,
    pub head_sha: String,
    pub base_sha: String,
    pub item_sha256: String,
    pub policy_sha256: String,
    pub reason: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evidence_sha: Option<String>,
}

/// Status of an evaluated disposition comment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DispositionStatus {
    Accepted,
    Unauthorized,
    StaleHead,
    MismatchedItemHash,
    MissingEvidence,
    InvalidSchema,
}

/// A parsed disposition entry with authorization audit metadata.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DispositionEntry {
    pub record: DispositionRecord,
    pub author: String,
    pub comment_id: Option<u64>,
    pub authorized: bool,
    pub status: DispositionStatus,
}

/// Item type in the conversation ledger.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ItemType {
    Review,
    ReviewThread,
    ReviewComment,
    IssueComment,
}

impl ItemType {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Review => "review",
            Self::ReviewThread => "review-thread",
            Self::ReviewComment => "review-comment",
            Self::IssueComment => "issue-comment",
        }
    }
}

/// A single item in the complete conversation ledger.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationItem {
    pub item_ref: String,
    pub item_type: ItemType,
    pub id: String,
    pub author: String,
    pub url: Option<String>,
    pub created_at: String,
    pub updated_at: Option<String>,
    pub body: String,
    pub body_hash: String,
    pub commit_oid: Option<String>,
    pub state: String,
    pub actionable: bool,
}

/// Unresolved or unassessed item requiring action or assessment.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NeedsTriageItem {
    pub item_ref: String,
    pub author: String,
    pub excerpt: String,
    pub reason: String,
}

/// Evaluated conversation summary emitted in readiness reports.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationLedger {
    pub review_decision: Option<String>,
    pub items: Vec<ConversationItem>,
    pub accepted_dispositions: Vec<DispositionRecord>,
    pub rejected_dispositions: Vec<DispositionEntry>,
    pub needs_triage: Vec<NeedsTriageItem>,
    pub inventory_digest: String,
    pub complete: bool,
}
