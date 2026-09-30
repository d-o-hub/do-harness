//! Fixture: the residue shapes PR #1042 waived (see ../README.md).
//!
//! Extracted from `memory-core/src/retrieval/rerank.rs` at
//! `69e50f3592e7fb95ae0a3d29c023f35905e1b282`.

use tracing::{debug, info};

/// Rerank outcome for the caller.
#[derive(Debug)]
pub enum RerankStatus {
    /// Reranking succeeded.
    Ok,
    /// The provider response violated the contract.
    Invalid,
    /// No judge is configured for this call.
    NotConfigured,
}

/// Provider status used by the visibility log.
pub enum Status {
    /// The provider answered.
    Ok,
}

impl Status {
    /// Stable lowercase label.
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Ok => "ok",
        }
    }
}

/// Provider report used by the visibility log.
pub struct Report {
    /// Provider latency in milliseconds.
    pub provider_ms: u64,
    /// Relevance confidence in `[0, 1]`.
    pub avg_confidence: f32,
    /// Shortlist size the provider saw.
    pub outcome: Outcome,
}

/// Provider outcome counters.
pub struct Outcome {
    /// Candidates the shortlist carried.
    pub shortlist_len: usize,
    /// Candidates the provider returned.
    pub output_len: usize,
    /// Whether the top candidate changed.
    pub top1_changed: bool,
}

/// One rerank judgment.
pub struct Judgment {
    /// Candidate the judgment refers to.
    pub id: String,
}

/// Provider failure.
pub enum JudgeError {
    /// The provider timed out.
    Timeout,
    /// The provider response violated the contract.
    Invalid,
}

/// Applies the rerank arm to the provider result.
///
/// # Errors
///
/// Returns a status when the provider result cannot be used.
pub fn apply_rerank(
    report: &Report,
    status: Status,
    shortlist_len: usize,
    result: Result<Option<Vec<Judgment>>, JudgeError>,
    provider_ms: u64,
) -> Result<(Vec<Judgment>, u64), (RerankStatus, u64)> {
    if shortlist_len == 0 {
        return Err((RerankStatus::Ok, provider_ms));
    }

    info!(
        status = %status.as_str(),
        shortlist_len = report.outcome.shortlist_len,
        output_len = report.outcome.output_len,
        top1_changed = report.outcome.top1_changed,
        provider_ms = report.provider_ms,
        avg_relevance_confidence = f64::from(report.avg_confidence),
        "semantic rerank finished"
    );

    match result {
        Ok(Some(judgments)) if judgments.len() == shortlist_len => Ok((judgments, provider_ms)),
        Ok(Some(judgments)) => {
            let count = judgments.len();
            debug!(
                shortlist_len = shortlist_len,
                judgment_count = count,
                "count mismatch"
            );
            Err((RerankStatus::Invalid, provider_ms))
        }
        // Unreachable while a judge is configured: never invent a ranking.
        Ok(None) => Err((RerankStatus::NotConfigured, provider_ms)),
        Err(err) => {
            let status = match err {
                JudgeError::Timeout => RerankStatus::NotConfigured,
                JudgeError::Invalid => RerankStatus::Invalid,
            };
            Err((status, provider_ms))
        }
    }
}
