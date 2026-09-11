//! Workspace and policy fingerprints for evidence freshness.
//!
//! The workspace fingerprint captures the working-tree state a run observed
//! (staged, unstaged, deleted, renamed, and relevant untracked files by
//! content hash — never mtimes), so an uncommitted edit invalidates prior
//! evidence even when `HEAD` is unchanged. The policy fingerprint captures
//! the effective verification policy, so a `do-harness.toml` change
//! invalidates prior evidence even when the tree is clean.

use std::path::Path;

use sha2::{Digest, Sha256};

use crate::changes::{ChangeKind, ChangedFiles};
use crate::config::{Config, SensorSpec};

/// Prefix marking harness-owned state excluded from the workspace manifest.
///
/// Evidence artifacts, the state database, and other `.do-harness/` contents
/// are produced by verification itself; fingerprinting them would make every
/// run instantly stale. Gitignored build outputs never enter the manifest
/// because change discovery (`git status`) omits them.
const HARNESS_STATE_PREFIX: &str = ".do-harness/";

/// Marker hashed when repository state cannot be determined.
const DISCOVERY_FAILED_MARKER: &str = "git-unavailable";

/// Marker hashed for the built-in pack when no config file exists.
const BUILTIN_CONFIG_MARKER: &str = "builtin-default";

/// One manifest entry: a changed path and its content identity.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
struct ManifestEntry {
    /// Repository-relative path (`/`-separated).
    path: String,
    /// Hex content hash, `"deleted"`, or `"unreadable"`.
    digest: String,
}

/// Workspace and policy fingerprints for one verify run or status check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fingerprints {
    /// Content hash of the working-tree manifest (`sha256:…`).
    pub workspace: String,
    /// Content hash of the effective run policy (`sha256:…`).
    pub policy: String,
    /// Content hash of the policy without the signal-set name (`sha256:…`),
    /// enabling coverage checks of stronger evidence for a weaker set.
    pub config: String,
}

/// Computes fingerprints for a run over `candidates` in set `set`.
///
/// `config_bytes` are the raw `do-harness.toml` bytes (`None` for the
/// built-in pack). `changed` is the post-execution working-tree state:
/// fingerprints describe the workspace the run leaves behind, because
/// sensors may create files and status never executes sensors.
///
/// Harness-owned `.do-harness/` paths are excluded from the manifest.
#[must_use]
pub fn for_run(
    root: &Path,
    cfg: &Config,
    config_bytes: Option<&[u8]>,
    set: Option<&str>,
    candidates: &[&SensorSpec],
    changed: &ChangedFiles,
) -> Fingerprints {
    Fingerprints {
        workspace: workspace_fingerprint(root, changed),
        policy: policy_fingerprint(cfg, config_bytes, set, candidates),
        config: config_fingerprint(cfg, config_bytes, candidates),
    }
}

/// Content hash of the working-tree manifest (`sha256:…`).
///
/// Deterministic across repeated calls on unchanged content: entries are
/// sorted by path and hash file bytes, never metadata. Deletions hash as a
/// marker so removing a file changes the fingerprint.
#[must_use]
pub fn workspace_fingerprint(root: &Path, changed: &ChangedFiles) -> String {
    if changed.discovery_failed {
        return hash_bytes(DISCOVERY_FAILED_MARKER.as_bytes());
    }
    let mut entries: Vec<ManifestEntry> = changed
        .files
        .iter()
        .filter(|file| !is_harness_state(&file.path))
        .map(|file| ManifestEntry {
            path: file.path.clone(),
            digest: file_digest(root, file),
        })
        .collect();
    entries.sort_by(|a, b| a.path.cmp(&b.path));
    let manifest = serde_json::json!({ "files": entries });
    hash_canonical(&manifest)
}

/// Content identity of one manifest file: hex hash, `"deleted"`, or
/// `"unreadable"` when the file cannot be read.
fn file_digest(root: &Path, file: &crate::changes::ChangedFile) -> String {
    if file.kind == ChangeKind::Deleted {
        return "deleted".to_owned();
    }
    match std::fs::read(root.join(&file.path)) {
        Ok(bytes) => hex::encode(Sha256::digest(&bytes)),
        Err(_) => "unreadable".to_owned(),
    }
}

/// Effective run policy hash (`sha256:…`).
///
/// Covers the raw config bytes (or the built-in marker), the harness
/// version, the selected signal set, and the full sensor definitions of the
/// set — including applicability rules — so any policy edit invalidates old
/// evidence.
#[must_use]
pub fn policy_fingerprint(
    cfg: &Config,
    config_bytes: Option<&[u8]>,
    set: Option<&str>,
    candidates: &[&SensorSpec],
) -> String {
    let payload = serde_json::json!({
        "config": config_digest(config_bytes),
        "harness_version": env!("CARGO_PKG_VERSION"),
        "language": cfg.language,
        "signal_set": set,
        "sensors": candidates.iter().copied().map(policy_sensor).collect::<Vec<_>>(),
    });
    hash_canonical(&payload)
}

/// Policy hash without the signal-set name (`sha256:…`).
#[must_use]
pub fn config_fingerprint(
    cfg: &Config,
    config_bytes: Option<&[u8]>,
    candidates: &[&SensorSpec],
) -> String {
    let payload = serde_json::json!({
        "config": config_digest(config_bytes),
        "harness_version": env!("CARGO_PKG_VERSION"),
        "language": cfg.language,
        "sensors": candidates.iter().copied().map(policy_sensor).collect::<Vec<_>>(),
    });
    hash_canonical(&payload)
}

/// Canonical JSON view of one sensor definition for policy hashing.
fn policy_sensor(spec: &SensorSpec) -> serde_json::Value {
    serde_json::json!({
        "name": spec.name,
        "argv": spec.argv,
        "retry": spec.retry,
        "timeout": spec.timeout,
        "allow_failure": spec.allow_failure,
        "transient_exit_codes": spec.transient_exit_codes,
        "when_changed": spec.when_changed,
    })
}

/// Config identity: hex of the raw bytes, or the built-in marker.
fn config_digest(config_bytes: Option<&[u8]>) -> String {
    match config_bytes {
        Some(bytes) => hex::encode(Sha256::digest(bytes)),
        None => BUILTIN_CONFIG_MARKER.to_owned(),
    }
}

/// Whether a manifest path is harness-owned state (excluded from hashing).
fn is_harness_state(path: &str) -> bool {
    path == ".do-harness" || path.starts_with(HARNESS_STATE_PREFIX)
}

/// `sha256:`-prefixed hex of raw bytes.
fn hash_bytes(bytes: &[u8]) -> String {
    format!("sha256:{}", hex::encode(Sha256::digest(bytes)))
}

/// `sha256:`-prefixed hex of the canonical JSON encoding.
fn hash_canonical(value: &serde_json::Value) -> String {
    match do_harness_types::canonical_value(value) {
        Ok(canonical) => {
            let bytes = serde_json::to_vec(&canonical).unwrap_or_default();
            hash_bytes(&bytes)
        }
        Err(_) => hash_bytes(&serde_json::to_vec(value).unwrap_or_default()),
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use crate::changes::{ChangeKind, ChangedFile};

    /// Builds a minimal config with one sensor for fingerprint tests.
    fn config() -> Config {
        Config {
            language: None,
            hooks: crate::config::HooksConfig::default(),
            signal_sets: std::collections::BTreeMap::new(),
            sensors: vec![SensorSpec {
                name: "a".to_owned(),
                argv: vec!["true".to_owned()],
                retry: None,
                timeout: None,
                allow_failure: false,
                transient_exit_codes: Vec::new(),
                when_changed: Vec::new(),
            }],
        }
    }

    /// Builds a changed-files set from (path, kind) pairs.
    fn changed(entries: &[(&str, ChangeKind)]) -> ChangedFiles {
        ChangedFiles {
            files: entries
                .iter()
                .map(|(path, kind)| ChangedFile {
                    path: (*path).to_owned(),
                    kind: kind.clone(),
                })
                .collect(),
            discovery_failed: false,
        }
    }

    /// An uncommitted edit changes the workspace fingerprint; repeats match.
    #[test]
    fn edit_changes_workspace_fingerprint() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("lib.rs"), "v1\n").unwrap();
        let before =
            workspace_fingerprint(dir.path(), &changed(&[("lib.rs", ChangeKind::Untracked)]));
        let repeat =
            workspace_fingerprint(dir.path(), &changed(&[("lib.rs", ChangeKind::Untracked)]));
        assert_eq!(before, repeat, "fingerprint must be deterministic");

        std::fs::write(dir.path().join("lib.rs"), "v2\n").unwrap();
        let after =
            workspace_fingerprint(dir.path(), &changed(&[("lib.rs", ChangeKind::Untracked)]));
        assert_ne!(before, after, "edit must change the fingerprint");
        assert!(after.starts_with("sha256:"));
    }

    /// Harness-owned `.do-harness/` paths never enter the manifest.
    #[test]
    fn harness_state_is_excluded() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("lib.rs"), "v1\n").unwrap();
        let without =
            workspace_fingerprint(dir.path(), &changed(&[("lib.rs", ChangeKind::Untracked)]));
        std::fs::create_dir_all(dir.path().join(".do-harness")).unwrap();
        std::fs::write(dir.path().join(".do-harness/evidence.json"), "{}\n").unwrap();
        let with = workspace_fingerprint(
            dir.path(),
            &changed(&[
                ("lib.rs", ChangeKind::Untracked),
                (".do-harness/evidence.json", ChangeKind::Untracked),
            ]),
        );
        assert_eq!(without, with);
    }

    /// A config byte change alters the policy fingerprint.
    #[test]
    fn config_bytes_change_policy_fingerprint() {
        let cfg = config();
        let candidates: Vec<&SensorSpec> = cfg.sensors.iter().collect();
        let first = policy_fingerprint(&cfg, Some(b"a = 1\n"), Some("verification"), &candidates);
        let same = policy_fingerprint(&cfg, Some(b"a = 1\n"), Some("verification"), &candidates);
        assert_eq!(first, same);
        let second = policy_fingerprint(
            &cfg,
            Some(b"a = 1\n# tweak\n"),
            Some("verification"),
            &candidates,
        );
        assert_ne!(first, second, "config bytes must feed the policy hash");
        // The set-free config fingerprint ignores the signal-set name.
        let free_a = config_fingerprint(&cfg, Some(b"a = 1\n"), &candidates);
        assert_ne!(
            first, free_a,
            "policy and config fingerprints must differ structurally"
        );
    }

    /// Discovery failure yields a stable marker, never a crash.
    #[test]
    fn discovery_failure_hashes_marker() {
        let dir = tempfile::tempdir().unwrap();
        let failed = ChangedFiles {
            files: Vec::new(),
            discovery_failed: true,
        };
        let first = workspace_fingerprint(dir.path(), &failed);
        let second = workspace_fingerprint(dir.path(), &failed);
        assert_eq!(first, second);
        assert!(first.starts_with("sha256:"));
    }
}
