//! Evidence artifact lookup and coverage checks for `status`.

use std::path::{Path, PathBuf};

use crate::evidence::EvidenceDocument;
use crate::status::default_path_for_set;

/// How the evidence search resolved.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Coverage {
    /// A qualifying v3 artifact was found.
    Found,
    /// Only pre-fingerprint (schema v2 or older) artifacts exist.
    Legacy,
    /// An artifact exists but covers a different set.
    SetMismatch,
    /// No artifact exists at all.
    None,
}

/// Candidate evidence paths: explicit override, per-set default, or legacy.
pub(super) fn evidence_paths(
    root: &Path,
    set: Option<&str>,
    evidence_override: Option<&Path>,
) -> Vec<PathBuf> {
    if let Some(path) = evidence_override {
        return vec![if path.is_relative() {
            root.join(path)
        } else {
            path.to_path_buf()
        }];
    }
    match set {
        Some(name) => vec![default_path_for_set(root, name)],
        None => vec![root.join(".do-harness/evidence.json")],
    }
}

/// Finds the first artifact that qualifies for `set`.
///
/// The primary path wins when it parses as v3 and covers the required
/// sensors (exact set match, or a stronger set sharing the config
/// fingerprint). Otherwise sibling per-set artifacts are scanned for
/// covering evidence, so a stronger set can satisfy a weaker contract.
pub(super) fn find_covering_evidence(
    paths: &[PathBuf],
    set: Option<&str>,
    required: &[String],
) -> (Option<EvidenceDocument>, Coverage) {
    let mut coverage = Coverage::None;
    for path in paths {
        let Ok(bytes) = std::fs::read(path) else {
            continue;
        };
        match serde_json::from_slice::<EvidenceDocument>(&bytes) {
            Ok(document) => {
                if qualifies(&document, set, required) {
                    return (Some(document), Coverage::Found);
                }
                coverage = Coverage::SetMismatch;
            }
            Err(_) => {
                if is_legacy_evidence(&bytes) {
                    coverage = Coverage::Legacy;
                }
            }
        }
    }
    if set.is_none() {
        return (None, coverage);
    }
    // Fallback: a stronger per-set artifact can satisfy a weaker contract.
    let Some(dir) = paths[0].parent() else {
        return (None, coverage);
    };
    let Ok(entries) = std::fs::read_dir(dir) else {
        return (None, coverage);
    };
    let mut siblings: Vec<PathBuf> = entries
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .filter(|path| {
            path.extension().is_some_and(|ext| ext == "json")
                && path
                    .file_stem()
                    .and_then(|stem| stem.to_str())
                    .is_some_and(|stem| stem.starts_with("evidence.") && *path != paths[0])
        })
        .collect();
    siblings.sort();
    for path in siblings {
        let Ok(bytes) = std::fs::read(&path) else {
            continue;
        };
        if let Ok(document) = serde_json::from_slice::<EvidenceDocument>(&bytes)
            && qualifies(&document, set, required)
        {
            return (Some(document), Coverage::Found);
        }
    }
    (None, coverage)
}

/// Whether `document` can satisfy `set`: exact set match, or a config-share
/// superset covering every required sensor. Freshness (red vs stale) is
/// decided by the caller from the fingerprints.
pub(super) fn qualifies(
    document: &EvidenceDocument,
    set: Option<&str>,
    required: &[String],
) -> bool {
    if document.schema_version != crate::evidence::EVIDENCE_SCHEMA_VERSION {
        return false;
    }
    if document.signal_set.as_deref() == set {
        // Same-set evidence qualifies for freshness comparison even when it
        // records failures (red) or is stale; coverage of the required
        // sensors is checked by the caller.
        return true;
    }
    let passed: Vec<&str> = document
        .sensors
        .iter()
        .filter(|s| s.verdict == "pass")
        .map(|s| s.name.as_str())
        .collect();
    required.iter().all(|name| passed.iter().any(|p| p == name))
}

/// Whether raw bytes look like a pre-fingerprint evidence artifact.
pub(super) fn is_legacy_evidence(bytes: &[u8]) -> bool {
    serde_json::from_slice::<serde_json::Value>(bytes)
        .ok()
        .and_then(|value| {
            value
                .get("schema_version")
                .and_then(serde_json::Value::as_u64)
        })
        .is_some_and(|version| version < u64::from(crate::evidence::EVIDENCE_SCHEMA_VERSION))
}
