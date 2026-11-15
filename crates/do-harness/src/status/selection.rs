//! Selection-revision freshness for range-based evidence.
//!
//! A `--since` artifact only describes the revisions it ran against: a new
//! commit or an advanced base branch changes the diff even when the working
//! tree and policy are untouched.

use std::path::{Path, PathBuf};

use super::{CurrentFacts, CurrentSide};
use crate::{config, evidence::EvidenceDocument};

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
    if facts.explicit_range {
        let Some(recorded) = &document.selection else {
            return false;
        };
        let Some(current) = &facts.range else {
            return false;
        };
        return recorded.mode == "range"
            && recorded.since.as_deref() == Some(current.requested.as_str())
            && recorded.base_sha.as_deref() == Some(current.base_sha.as_str())
            && recorded.head_sha.as_deref() == Some(current.head_sha.as_str())
            && recorded.merge_base_sha.as_deref() == Some(current.merge_base_sha.as_str());
    }
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
impl CurrentFacts {
    /// Recomputes fingerprints and the required sensor list (no sensors run).
    pub(super) fn compute(
        root: &Path,
        cfg: &config::Config,
        config_bytes: Option<&[u8]>,
        set: Option<&str>,
        since: Option<&str>,
        explicit_range: bool,
    ) -> anyhow::Result<Self> {
        let candidates = crate::signals::candidate_specs(cfg, set)?;
        // Unfiltered evidence fingerprints the working tree, as it always
        // has; range evidence must be judged against the same range.
        let selection = if let Some(since) = since {
            crate::changes::discover_selection(root, false, Some(since))
        } else {
            let changed = crate::changes::discover(root);
            crate::changes::ChangeSelection {
                mode: crate::changes::SelectionMode::All,
                changed,
                range: None,
            }
        };
        if explicit_range {
            if let Some(error) = &selection.changed.discovery_error {
                anyhow::bail!("{error}");
            }
            if selection.changed.files.is_empty() {
                anyhow::bail!(
                    "no change input: pass --since <REF> for committed changes or run verify without change filtering"
                );
            }
        }
        let changed = &selection.changed;
        let range = selection.range.clone();
        let applicability = crate::applicability::select(&candidates, changed);
        let required = applicability.selected_names();
        let fingerprints =
            crate::fingerprint::for_run(root, cfg, config_bytes, set, &candidates, changed);
        let workspace = fingerprints.workspace;
        let policy = fingerprints.policy;
        let config = fingerprints.config;
        let manifest = crate::provenance::InputManifest::capture(
            root,
            &candidates,
            config_bytes,
            Some(&selection),
        );
        Ok(Self {
            side: CurrentSide {
                workspace_fingerprint: workspace.clone(),
                policy_fingerprint: policy.clone(),
                failed: Vec::new(),
                input_mismatches: manifest
                    .sensors
                    .iter()
                    .filter(|(_, digest)| digest.is_none())
                    .map(|(name, _)| name.clone())
                    .collect(),
            },
            required,
            workspace,
            policy,
            config,
            range,
            manifest,
            explicit_range,
        })
    }
}
