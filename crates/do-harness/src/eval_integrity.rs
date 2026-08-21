//! Grader tamper-evidence and the pass-rate bar ratchet for `do-harness eval`.
//!
//! Verifier separation: the agent may edit skills, but the graders that judge
//! them are baselined by hash at bless time. Any drift between the blessed
//! hashes and the on-disk `walkthrough.sh` / `evals.json` fails the eval until
//! a human reviews the change and re-blesses. The bar ratchet complements this
//! by never letting a skill's pass-rate floor drop once blessed.

use std::path::Path;

use anyhow::{Context, Result};
use sha2::{Digest, Sha256};

use do_harness_types::GraderBaseline;

/// How far below the best recorded pass rate a skill may fall before the
/// blessed bar fails it.
pub const BAR_TOLERANCE: f64 = 0.05;

/// SHA-256 digests of a skill's grader files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraderHashes {
    /// Hash of `evals/walkthrough.sh` (empty-string hash when absent).
    pub walkthrough_sha: String,
    /// Hash of `evals/evals.json`.
    pub specs_sha: String,
}

impl GraderHashes {
    /// Whether both hashes match a [`GraderBaseline`].
    #[must_use]
    pub fn matches_baseline(&self, baseline: &GraderBaseline) -> bool {
        self.walkthrough_sha == baseline.walkthrough_sha && self.specs_sha == baseline.specs_sha
    }

    /// The bar floor implied by the best-ever pass rate: `max - tolerance`,
    /// clamped to `[0, 1]`.
    #[must_use]
    pub fn bar_floor(best_ever: Option<f64>) -> Option<f64> {
        best_ever.map(|best| (best - BAR_TOLERANCE).clamp(0.0, 1.0))
    }
}

/// Computes the grader hashes for a skill directory.
///
/// Missing grader files (no `evals.json`, no `walkthrough.sh`) hash as
/// empty, so a baseline can also pin their absence.
///
/// # Errors
///
/// Returns an error when an existing grader file cannot be read.
pub fn grader_hashes(skill_dir: &Path) -> Result<GraderHashes> {
    let walkthrough = skill_dir.join("evals/walkthrough.sh");
    let walkthrough_bytes = match std::fs::read(&walkthrough) {
        Ok(bytes) => bytes,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(err) => {
            return Err(err).context(format!("failed to read {}", walkthrough.display()));
        }
    };
    let specs_path = skill_dir.join("evals/evals.json");
    let specs_bytes = match std::fs::read(&specs_path) {
        Ok(bytes) => bytes,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(err) => {
            return Err(err).context(format!("failed to read {}", specs_path.display()));
        }
    };
    Ok(GraderHashes {
        walkthrough_sha: hex_sha256(&walkthrough_bytes),
        specs_sha: hex_sha256(&specs_bytes),
    })
}

/// Lowercase hex encoding of the SHA-256 digest of `bytes`.
fn hex_sha256(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let digest = Sha256::digest(bytes);
    digest
        .iter()
        .flat_map(|byte| {
            [
                HEX[usize::from(byte >> 4)] as char,
                HEX[usize::from(byte & 0x0F)] as char,
            ]
        })
        .collect()
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    fn write_skill(dir: &Path, walkthrough: Option<&str>, specs: &str) -> std::path::PathBuf {
        let evals = dir.join("evals");
        std::fs::create_dir_all(&evals).unwrap();
        if let Some(script) = walkthrough {
            std::fs::write(evals.join("walkthrough.sh"), script).unwrap();
        }
        std::fs::write(evals.join("evals.json"), specs).unwrap();
        dir.to_path_buf()
    }

    #[test]
    fn hashes_are_deterministic_and_content_sensitive() {
        let dir = tempfile::tempdir().unwrap();
        let skill = write_skill(
            dir.path(),
            Some("#!/bin/sh\nexit 0\n"),
            "{\"skill_name\":\"x\",\"evals\":[]}",
        );
        let first = grader_hashes(&skill).unwrap();
        let again = grader_hashes(&skill).unwrap();
        assert_eq!(first, again);
        assert_eq!(first.walkthrough_sha.len(), 64);

        std::fs::write(skill.join("evals/walkthrough.sh"), "#!/bin/sh\nexit 1\n").unwrap();
        let changed = grader_hashes(&skill).unwrap();
        assert_ne!(first.walkthrough_sha, changed.walkthrough_sha);
        assert_eq!(first.specs_sha, changed.specs_sha);
    }

    #[test]
    fn missing_walkthrough_hashes_as_empty() {
        let dir = tempfile::tempdir().unwrap();
        let skill = write_skill(dir.path(), None, "{}");
        let hashes = grader_hashes(&skill).unwrap();
        assert_eq!(hashes.walkthrough_sha, hex_sha256(&[]));
    }

    #[test]
    fn matches_baseline_compares_both_hashes() {
        let baseline = GraderBaseline {
            skill_name: "harness".to_owned(),
            walkthrough_sha: "aaa".to_owned(),
            specs_sha: "bbb".to_owned(),
            blessed_at: 0,
        };
        let ok = GraderHashes {
            walkthrough_sha: "aaa".to_owned(),
            specs_sha: "bbb".to_owned(),
        };
        assert!(ok.matches_baseline(&baseline));
        let drifted = GraderHashes {
            walkthrough_sha: "zzz".to_owned(),
            specs_sha: "bbb".to_owned(),
        };
        assert!(!drifted.matches_baseline(&baseline));
    }

    #[test]
    fn bar_floor_applies_tolerance_and_clamps() {
        assert_eq!(GraderHashes::bar_floor(Some(1.0)), Some(0.95));
        assert_eq!(GraderHashes::bar_floor(Some(0.5)), Some(0.45));
        // A low best-ever clamps at zero rather than going negative.
        assert_eq!(GraderHashes::bar_floor(Some(0.02)), Some(0.0));
        assert_eq!(GraderHashes::bar_floor(None), None);
    }
}
