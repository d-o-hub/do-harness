//! Selection-revision freshness for range-based evidence.
//!
//! A `--since` artifact only describes the revisions it ran against: a new
//! commit or an advanced base branch changes the diff even when the working
//! tree and policy are untouched.

use std::path::PathBuf;

use super::CurrentFacts;
use crate::evidence::EvidenceDocument;

/// `--since` ref recorded by the primary evidence artifact, when it used one.
pub(super) fn recorded_since(paths: &[PathBuf]) -> Option<String> {
    paths.iter().find_map(|path| {
        let bytes = crate::managed_path::read_evidence(path).ok()?;
        let document: EvidenceDocument = serde_json::from_slice(&bytes).ok()?;
        document.selection.and_then(|selection| selection.since)
    })
}

/// Whether the recorded selection revisions still describe the current input.
pub(super) fn matches(document: &EvidenceDocument, facts: &CurrentFacts) -> bool {
    let Some(recorded) = &document.selection else {
        return true;
    };
    if recorded.since.is_none() {
        return true;
    }
    let Some(current) = &facts.range else {
        return false;
    };
    recorded.base_sha.as_deref() == Some(current.base_sha.as_str())
        && recorded.head_sha.as_deref() == Some(current.head_sha.as_str())
        && recorded.merge_base_sha.as_deref() == Some(current.merge_base_sha.as_str())
}
