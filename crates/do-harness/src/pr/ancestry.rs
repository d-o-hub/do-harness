//! Read-only ancestry proof for a captured `(base, head)` commit pair.
//!
//! A PR's `mergeable: MERGEABLE` and `mergeStateStatus: CLEAN` say nothing
//! about whether the head actually contains the base commit, and both are
//! computed from mutable refs. This module asks `GitHub` to compare two
//! immutable commit SHAs and requires the merge base to *be* the base commit,
//! which is the property a fast-forward-shaped merge depends on.

use std::path::Path;

use anyhow::{Context, Result};
use serde::Deserialize;

/// Verdict of the ancestry proof.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AncestryDecision {
    /// The head is strictly ahead of the base and the base is its merge base.
    Ahead,
    /// The base and head are the same commit.
    Identical,
    /// The head does not contain the base: the PR is not mergeable onto it.
    Diverged,
    /// The comparison could not be read or was incomplete.
    Unknown,
}

impl AncestryDecision {
    /// Stable lowercase code used in receipt and reason fields.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ahead => "ahead",
            Self::Identical => "identical",
            Self::Diverged => "diverged",
            Self::Unknown => "unknown",
        }
    }

    /// Whether this proof authorizes a merge.
    #[must_use]
    pub fn is_ancestor(self) -> bool {
        matches!(self, Self::Ahead | Self::Identical)
    }
}

/// Proof of the relationship between two captured commits.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AncestryProof {
    /// Verdict.
    pub decision: AncestryDecision,
    /// Base SHA the caller asked about.
    pub base_sha: String,
    /// Head SHA the caller asked about.
    pub head_sha: String,
    /// `status` reported by the compare API.
    pub status: String,
    /// `base_commit.sha` reported by the compare API.
    pub base_commit_sha: String,
    /// `merge_base_commit.sha` reported by the compare API.
    pub merge_base_sha: String,
    /// Human-readable explanation when the proof is not an ancestor.
    pub reason: Option<String>,
}

#[derive(Deserialize)]
struct CompareResponse {
    #[serde(default)]
    status: String,
    base_commit: Option<CompareCommit>,
    merge_base_commit: Option<CompareCommit>,
}

#[derive(Deserialize)]
struct CompareCommit {
    #[serde(default)]
    sha: String,
}

impl AncestryProof {
    fn unknown(base_sha: &str, head_sha: &str, reason: impl Into<String>) -> Self {
        Self {
            decision: AncestryDecision::Unknown,
            base_sha: base_sha.to_owned(),
            head_sha: head_sha.to_owned(),
            status: String::new(),
            base_commit_sha: String::new(),
            merge_base_sha: String::new(),
            reason: Some(reason.into()),
        }
    }
}

/// Reads the compare API for `base...head` and derives the ancestry verdict.
///
/// Both values must be full 40-character commit SHAs: a branch name is a
/// mutable ref and cannot prove anything about the commit being merged.
///
/// # Errors
///
/// Returns an error only when the compare request itself fails; a missing body
/// is reported as [`AncestryDecision::Unknown`] so the caller can distinguish
/// "we could not ask" from "the answer is no".
pub fn compare(root: &Path, base_sha: &str, head_sha: &str) -> Result<AncestryProof> {
    if !is_commit_sha(base_sha) || !is_commit_sha(head_sha) {
        return Ok(AncestryProof::unknown(
            base_sha,
            head_sha,
            "ancestry proof needs full 40-character commit SHAs for base and head",
        ));
    }

    let endpoint = format!("repos/{{owner}}/{{repo}}/compare/{base_sha}...{head_sha}");
    let output = crate::gh::output(
        root,
        &[
            "api",
            &endpoint,
            "-H",
            "Accept: application/vnd.github+json",
        ],
        "api compare",
    )
    .context("compare api request failed")?;

    let parsed: CompareResponse = match serde_json::from_slice(&output.stdout) {
        Ok(parsed) => parsed,
        Err(err) => {
            return Ok(AncestryProof::unknown(
                base_sha,
                head_sha,
                format!("compare payload was not valid JSON: {err}"),
            ));
        }
    };

    let base_commit_sha = parsed.base_commit.map_or_else(String::new, |c| c.sha);
    let merge_base_sha = parsed.merge_base_commit.map_or_else(String::new, |c| c.sha);

    if parsed.status.is_empty() || base_commit_sha.is_empty() || merge_base_sha.is_empty() {
        return Ok(AncestryProof::unknown(
            base_sha,
            head_sha,
            "compare payload omitted status, base_commit or merge_base_commit",
        ));
    }

    // `ahead`/`identical` is necessary but not sufficient: the compare API must
    // also confirm the captured base *is* the base commit and the merge base.
    let agrees = base_commit_sha == base_sha && merge_base_sha == base_sha;
    let decision = match parsed.status.as_str() {
        "ahead" if agrees => AncestryDecision::Ahead,
        "identical" if agrees => AncestryDecision::Identical,
        _ => AncestryDecision::Diverged,
    };

    let reason = match decision {
        AncestryDecision::Diverged => Some(format!(
            "head is not based on the captured base: status={}, base_commit={}, merge_base={}",
            parsed.status, base_commit_sha, merge_base_sha
        )),
        _ => None,
    };

    Ok(AncestryProof {
        decision,
        base_sha: base_sha.to_owned(),
        head_sha: head_sha.to_owned(),
        status: parsed.status,
        base_commit_sha,
        merge_base_sha,
        reason,
    })
}

/// Whether `value` is a full lowercase or uppercase 40-character hex SHA.
#[must_use]
pub fn is_commit_sha(value: &str) -> bool {
    value.len() == 40 && value.chars().all(|c| c.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commit_sha_validation_requires_forty_hex_characters() {
        assert!(is_commit_sha("03fda828085d72044965b6bf51c54e7e3a12d7cf"));
        assert!(is_commit_sha("03FDA828085D72044965B6BF51C54E7E3A12D7CF"));
        assert!(!is_commit_sha("main"));
        assert!(!is_commit_sha("03fda828"));
        assert!(!is_commit_sha("03fda828085d72044965b6bf51c54e7e3a12d7cg"));
        assert!(!is_commit_sha(""));
    }

    #[test]
    fn only_ahead_and_identical_authorize() {
        assert!(AncestryDecision::Ahead.is_ancestor());
        assert!(AncestryDecision::Identical.is_ancestor());
        assert!(!AncestryDecision::Diverged.is_ancestor());
        assert!(!AncestryDecision::Unknown.is_ancestor());
    }
}
