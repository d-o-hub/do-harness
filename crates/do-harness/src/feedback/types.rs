//! Typed contracts for impact-gated feedback and issue proposals (WF-09).

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Maximum excerpt length stored in a feedback draft.
pub const MAX_EXCERPT_BYTES: usize = 2048;

/// A local, impact-gated feedback draft.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FeedbackDraft {
    /// Schema version.
    pub schema_version: u32,
    /// Stable fingerprint for deduplication.
    pub fingerprint: String,
    /// Target repository (`OWNER/REPO`).
    pub target_repo: String,
    /// Issue title.
    pub title: String,
    /// Rendered issue body with opaque fingerprint marker.
    pub body: String,
    /// Business / harness impact rationale.
    pub impact: String,
    /// Build or commit identity.
    pub build_identity: String,
    /// Source beat or event IDs.
    pub source_ids: Vec<String>,
    /// Observed failure behavior.
    pub observed_behavior: String,
    /// Expected behavior.
    pub expected_behavior: String,
    /// Minimal reproduction steps or command.
    pub reproduction: String,
    /// Bounded, redacted error diagnostic excerpt.
    pub diagnostic_excerpt: String,
    /// Suggested verification command.
    pub suggested_verification: String,
    /// Unix timestamp when created.
    pub created_at: i64,
}

/// Local receipt for an issue submission or link.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SubmissionReceipt {
    /// Issue fingerprint.
    pub fingerprint: String,
    /// Target repository.
    pub target_repo: String,
    /// Remote issue number.
    pub issue_number: u64,
    /// Web URL of the issue.
    pub issue_url: String,
    /// Timestamp of receipt.
    pub timestamp: i64,
    /// Status (`created`, `linked_existing`).
    pub status: String,
}

/// Result of submitting a draft.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SubmissionResult {
    /// Newly created remote issue.
    Created {
        issue_number: u64,
        issue_url: String,
    },
    /// Deduplicated against existing open or closed issue.
    LinkedExisting {
        issue_number: u64,
        issue_url: String,
    },
    /// Simulation run without external side effects.
    DryRun {
        title: String,
        target_repo: String,
        would_create: bool,
    },
}

#[allow(dead_code)]
/// Command to generate local feedback drafts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DraftFeedbackCommand {
    pub target_repo: String,
    pub window_days: Option<i64>,
    pub min_fires: Option<u32>,
    pub out: Option<PathBuf>,
}

#[allow(dead_code)]
/// Command to submit an explicit feedback draft.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubmitFeedbackCommand {
    pub draft_path: PathBuf,
    pub target_repo: Option<String>,
    pub skip_confirmation: bool,
    pub dry_run: bool,
}

#[allow(dead_code)]
/// Domain event: a candidate was drafted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FeedbackCandidateDrafted {
    pub fingerprint: String,
    pub target_repo: String,
    pub title: String,
    pub fire_count: u32,
    pub created_at: i64,
}

#[allow(dead_code)]
/// Domain event: an issue was submitted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FeedbackSubmitted {
    pub fingerprint: String,
    pub target_repo: String,
    pub issue_number: u64,
    pub issue_url: String,
    pub submitted_at: i64,
}
