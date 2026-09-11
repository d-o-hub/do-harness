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

/// Maximum bytes hashed for a grader file; a larger file is rejected rather
/// than read into memory (baselines only need the content, not the whole tool).
const MAX_GRADER_BYTES: u64 = 1024 * 1024;

/// Computes the grader hashes for a skill directory.
///
/// Missing grader files hash as a distinct `absent` sentinel (not the empty
/// hash), so pinning absence cannot be confused with pinning an empty file.
/// The walkthrough hash also covers the executable bits: `chmod -x` changes
/// behavior (bash fallback) and must invalidate the baseline.
///
/// # Errors
///
/// Returns an error when an existing grader file cannot be read or exceeds
/// [`MAX_GRADER_BYTES`].
pub async fn grader_hashes(skill_dir: &Path) -> Result<GraderHashes> {
    Ok(GraderHashes {
        walkthrough_sha: hash_grader(&skill_dir.join("evals/walkthrough.sh")).await?,
        specs_sha: hash_grader(&skill_dir.join("evals/evals.json")).await?,
    })
}

/// Hashes one grader file with its executable bits in the digest.
async fn hash_grader(path: &Path) -> Result<String> {
    let metadata = match tokio::fs::metadata(path).await {
        Ok(metadata) => metadata,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            return Ok(sha256_hex(b"absent"));
        }
        Err(err) => {
            return Err(err).context(format!("failed to stat {}", path.display()));
        }
    };
    if metadata.len() > MAX_GRADER_BYTES {
        anyhow::bail!(
            "grader file {} is {} bytes (max {MAX_GRADER_BYTES})",
            path.display(),
            metadata.len()
        );
    }
    #[cfg(unix)]
    let mode = {
        use std::os::unix::fs::PermissionsExt as _;
        metadata.permissions().mode() & 0o111
    };
    #[cfg(not(unix))]
    let mode = 0u32;

    let bytes = tokio::fs::read(path)
        .await
        .context(format!("failed to read {}", path.display()))?;
    let mut hasher = Sha256::new();
    hasher.update(format!("mode:{mode:o}:len:{}:", bytes.len()).as_bytes());
    hasher.update(&bytes);
    Ok(hex::encode(hasher.finalize()))
}

/// Lowercase hex encoding of the SHA-256 digest of `bytes`.
fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
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

    #[tokio::test(flavor = "current_thread")]
    async fn hashes_are_deterministic_and_content_sensitive() {
        let dir = tempfile::tempdir().unwrap();
        let skill = write_skill(
            dir.path(),
            Some("#!/bin/sh\nexit 0\n"),
            "{\"skill_name\":\"x\",\"evals\":[]}",
        );
        let first = grader_hashes(&skill).await.unwrap();
        let again = grader_hashes(&skill).await.unwrap();
        assert_eq!(first, again);
        assert_eq!(first.walkthrough_sha.len(), 64);

        std::fs::write(skill.join("evals/walkthrough.sh"), "#!/bin/sh\nexit 1\n").unwrap();
        let changed = grader_hashes(&skill).await.unwrap();
        assert_ne!(first.walkthrough_sha, changed.walkthrough_sha);
        assert_eq!(first.specs_sha, changed.specs_sha);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn missing_walkthrough_hashes_as_absent_not_empty() {
        let dir = tempfile::tempdir().unwrap();
        let absent = write_skill(dir.path(), None, "{}");
        let hashes = grader_hashes(&absent).await.unwrap();
        assert_eq!(hashes.walkthrough_sha, sha256_hex(b"absent"));
        assert_ne!(hashes.walkthrough_sha, sha256_hex(b""));

        let empty = tempfile::tempdir().unwrap();
        write_skill(empty.path(), Some(""), "{}");
        let empty_hashes = grader_hashes(empty.path()).await.unwrap();
        assert_ne!(hashes.walkthrough_sha, empty_hashes.walkthrough_sha);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn oversized_grader_file_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let evals = dir.path().join("evals");
        std::fs::create_dir_all(&evals).unwrap();
        std::fs::write(evals.join("evals.json"), "{}").unwrap();
        std::fs::write(evals.join("walkthrough.sh"), vec![b'x'; 1024 * 1024 + 1]).unwrap();
        let err = grader_hashes(dir.path()).await.unwrap_err();
        assert!(err.to_string().contains("max"), "{err}");
    }

    #[cfg(unix)]
    #[tokio::test(flavor = "current_thread")]
    async fn executable_bits_change_walkthrough_hash() {
        use std::os::unix::fs::PermissionsExt as _;

        let dir = tempfile::tempdir().unwrap();
        let skill = write_skill(dir.path(), Some("#!/bin/sh\nexit 0\n"), "{}");
        let path = skill.join("evals/walkthrough.sh");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        let executable = grader_hashes(&skill).await.unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        let plain = grader_hashes(&skill).await.unwrap();
        assert_ne!(executable.walkthrough_sha, plain.walkthrough_sha);
        assert_eq!(executable.specs_sha, plain.specs_sha);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn matches_baseline_compares_both_hashes() {
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

    #[tokio::test(flavor = "current_thread")]
    async fn bar_floor_applies_tolerance_and_clamps() {
        assert_eq!(GraderHashes::bar_floor(Some(1.0)), Some(0.95));
        assert_eq!(GraderHashes::bar_floor(Some(0.5)), Some(0.45));
        // A low best-ever clamps at zero rather than going negative.
        assert_eq!(GraderHashes::bar_floor(Some(0.02)), Some(0.0));
        assert_eq!(GraderHashes::bar_floor(None), None);
    }
}
