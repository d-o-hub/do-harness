//! Fixture: the residue shapes PR #1041 waived (see ../README.md).
//!
//! Extracted from `memory-core/src/retrieval/judgment.rs` at
//! `b0be887f778ed9e4ab5b0884e356315b29fa1b42`.

use std::collections::{HashMap, HashSet};

use tracing::info;

/// Provider judgment outcome.
#[derive(Clone, Copy)]
pub enum JudgmentOutcome {
    /// The provider answered within the contract.
    Ok,
    /// The provider answered outside the contract.
    Invalid,
}

impl JudgmentOutcome {
    /// Stable lowercase label.
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::Invalid => "invalid",
        }
    }
}

/// A candidate the retrieval stage selected.
pub struct Candidate {
    /// Candidate ID.
    pub id: String,
}

/// A judgment returned by the provider.
pub struct CandidateJudgment {
    /// Candidate ID the judgment refers to.
    pub id: String,
}

/// Invalid provider response.
#[derive(Debug)]
pub struct JudgmentError(String);

impl JudgmentError {
    fn invalid(message: String) -> Self {
        Self(message)
    }
}

/// Re-aligns provider judgments to the input candidate order.
///
/// # Errors
///
/// Returns [`JudgmentError`] when the provider response violates the contract.
pub fn validate_and_align_judgments(
    candidates: &[Candidate],
    judgments: Vec<CandidateJudgment>,
) -> Result<Vec<CandidateJudgment>, JudgmentError> {
    let mut seen_ids = HashSet::with_capacity(judgments.len());
    for j in &judgments {
        if !seen_ids.insert(j.id.as_str()) {
            return Err(JudgmentError::invalid(format!(
                "duplicate candidate judgment ID in provider response: '{}'",
                j.id
            )));
        }
    }

    for c in candidates {
        if !seen_ids.contains(c.id.as_str()) {
            return Err(JudgmentError::invalid(format!(
                "missing candidate judgment ID in provider response: '{}'",
                c.id
            )));
        }
    }

    // Re-align judgments to match input candidate order. Every lookup
    // succeeds: the set check above rejects unknown or missing IDs and the
    // duplicate check rejects repeats, so each ID is consumed exactly once.
    let mut judgment_map: HashMap<String, CandidateJudgment> =
        judgments.into_iter().map(|j| (j.id.clone(), j)).collect();

    let mut aligned = Vec::with_capacity(candidates.len());
    for c in candidates {
        let Some(j) = judgment_map.remove(c.id.as_str()) else {
            return Err(JudgmentError::invalid(format!(
                "unknown candidate ID mismatch during alignment: '{}'",
                c.id
            )));
        };
        aligned.push(j);
    }

    Ok(aligned)
}

/// Evaluates a provider response and records telemetry.
///
/// # Errors
///
/// Returns [`JudgmentError`] when the provider response violates the contract.
pub fn evaluate_judgments(
    candidates: &[Candidate],
    raw: Result<Vec<CandidateJudgment>, JudgmentError>,
    elapsed_ms: u64,
) -> Result<Vec<CandidateJudgment>, JudgmentError> {
    match raw {
        Ok(raw_judgments) => match validate_and_align_judgments(candidates, raw_judgments) {
            Ok(aligned) => {
                info!(
                    judge_configured = true,
                    outcome = %JudgmentOutcome::Ok.as_str(),
                    candidate_count = candidates.len(),
                    elapsed_ms = elapsed_ms,
                    "semantic candidate judgment completed successfully"
                );
                Ok(aligned)
            }
            Err(err) => Err(err),
        },
        Err(err) => {
            info!(
                judge_configured = true,
                outcome = %JudgmentOutcome::Invalid.as_str(),
                candidate_count = candidates.len(),
                elapsed_ms = elapsed_ms,
                error = %err.0,
                "semantic candidate judgment rejected the provider response"
            );
            Err(err)
        }
    }
}
