//! Blessed findings ratchet baselines for `do-harness verify`.
//!
//! Baselines live in the committed `plans/baselines.json` so every clone and
//! CI job enforces the same ceilings. `verify --record --bless` lowers or
//! initializes an entry; a bless never raises a baseline. Sensors report
//! their findings count with a `FINDINGS: <n>` output marker.

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Repository-relative path of the committed baseline file.
pub const BASELINES_PATH: &str = "plans/baselines.json";

/// Marker hashed when no baseline file exists.
pub const ABSENT_MARKER: &str = "absent";

/// Blessed per-sensor findings ceilings.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Baselines {
    /// Sensor name to blessed maximum findings count.
    #[serde(default)]
    pub sensors: BTreeMap<String, u64>,
}

/// Outcome of blessing one sensor's observed findings count.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlessOutcome {
    /// No baseline existed; the observed count became the baseline.
    Initialized(u64),
    /// The baseline was lowered to the observed count.
    Lowered {
        /// Previous baseline.
        from: u64,
        /// New baseline.
        to: u64,
    },
    /// The observed count equals the baseline; nothing changed.
    Unchanged(u64),
    /// The observed count exceeds the baseline; a raise is refused.
    Refused {
        /// Current baseline.
        baseline: u64,
        /// Observed count that would have raised it.
        observed: u64,
    },
}

impl Baselines {
    /// Loads `<root>/plans/baselines.json`; a missing file means no ratchet.
    ///
    /// # Errors
    ///
    /// Returns an error when the file exists but cannot be read or parsed.
    pub async fn load(root: &Path) -> Result<Self> {
        let path = root.join(BASELINES_PATH);
        let Ok(bytes) = tokio::fs::read(&path).await else {
            return Ok(Self::default());
        };
        serde_json::from_slice(&bytes)
            .with_context(|| format!("invalid baseline file {}", path.display()))
    }

    /// Returns the blessed baseline for `name`, when one exists.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<u64> {
        self.sensors.get(name).copied()
    }

    /// Writes the baseline file with a trailing newline.
    ///
    /// # Errors
    ///
    /// Returns an error when the parent directory or file cannot be written.
    pub async fn save(&self, root: &Path) -> Result<()> {
        let path = root.join(BASELINES_PATH);
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
        let mut json = serde_json::to_vec_pretty(self)?;
        json.push(b'\n');
        tokio::fs::write(&path, json)
            .await
            .with_context(|| format!("failed to write {}", path.display()))
    }

    /// Lower-or-initialize ratchet for one sensor. Never raises.
    pub fn bless(&mut self, name: &str, observed: u64) -> BlessOutcome {
        match self.sensors.get(name).copied() {
            None => {
                self.sensors.insert(name.to_owned(), observed);
                BlessOutcome::Initialized(observed)
            }
            Some(baseline) if observed < baseline => {
                self.sensors.insert(name.to_owned(), observed);
                BlessOutcome::Lowered {
                    from: baseline,
                    to: observed,
                }
            }
            Some(baseline) if observed == baseline => BlessOutcome::Unchanged(baseline),
            Some(baseline) => BlessOutcome::Refused { baseline, observed },
        }
    }
}

/// Content digest of the baseline file for policy fingerprinting: hex SHA-256
/// of the raw bytes, or the absent marker when no file exists.
#[must_use]
pub fn digest(root: &Path) -> String {
    match std::fs::read(root.join(BASELINES_PATH)) {
        Ok(bytes) => hex::encode(Sha256::digest(&bytes)),
        Err(_) => ABSENT_MARKER.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    /// Bless initializes a missing entry, then only lowers it.
    #[test]
    fn bless_initializes_and_only_lowers() {
        let mut baselines = Baselines::default();
        assert_eq!(baselines.bless("ux", 12), BlessOutcome::Initialized(12));
        assert_eq!(baselines.get("ux"), Some(12));
        assert_eq!(baselines.bless("ux", 12), BlessOutcome::Unchanged(12));
        assert_eq!(
            baselines.bless("ux", 20),
            BlessOutcome::Refused {
                baseline: 12,
                observed: 20
            }
        );
        assert_eq!(baselines.get("ux"), Some(12), "a raise must be refused");
        assert_eq!(
            baselines.bless("ux", 3),
            BlessOutcome::Lowered { from: 12, to: 3 }
        );
        assert_eq!(baselines.get("ux"), Some(3));
    }

    /// The baseline file round-trips through disk and drives the digest.
    #[tokio::test(flavor = "current_thread")]
    async fn file_round_trip_and_digest() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(digest(dir.path()), ABSENT_MARKER);
        let before = digest(dir.path());

        let mut baselines = Baselines::default();
        baselines.bless("a11y", 7);
        baselines.save(dir.path()).await.unwrap();
        assert_ne!(digest(dir.path()), before);

        let loaded = Baselines::load(dir.path()).await.unwrap();
        assert_eq!(loaded.get("a11y"), Some(7));
        assert_eq!(loaded, baselines);
    }

    /// An unreadable baseline file fails closed instead of silently disabling
    /// the ratchet.
    #[tokio::test(flavor = "current_thread")]
    async fn invalid_file_fails_load() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(BASELINES_PATH);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "{ not json").unwrap();
        assert!(Baselines::load(dir.path()).await.is_err());
    }
}
