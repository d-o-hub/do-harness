//! Best-effort review cache under the repository git directory.
//!
//! The cache never lives in the work tree (the `pr-triage` skill owns
//! `.git/pr-triage/` for sweep state) and never affects the verdict: a
//! missing, unreadable, or stale entry simply forces recomputation.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::changes::git_command;

/// Values that identify one cached review run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CacheKey {
    /// Report schema version.
    pub schema_version: u32,
    /// Merge-base commit, or the base ref when no merge base is available.
    pub merge_base: String,
    /// Resolved head commit.
    pub head_sha: String,
    /// Policy fingerprint (hex sha256); `none` when the policy is absent.
    pub policy_sha256: String,
}

/// Reads the cached value for `key`, or `None` when absent or unreadable.
#[must_use]
pub fn load<T: DeserializeOwned>(root: &Path, key: &CacheKey) -> Option<T> {
    let path = entry_path(root, key).ok()?;
    let bytes = fs::read(path).ok()?;
    serde_json::from_slice(&bytes).ok()
}

/// Atomically writes `value` for `key`; callers treat failures as advisory.
///
/// # Errors
///
/// Returns an error when the git directory cannot be resolved or the entry
/// cannot be serialized and renamed into place.
pub fn store<T: Serialize>(root: &Path, key: &CacheKey, value: &T) -> Result<()> {
    let path = entry_path(root, key)?;
    let dir = path.parent().context("cache path has no parent")?;
    fs::create_dir_all(dir).with_context(|| format!("create {}", dir.display()))?;
    let bytes = serde_json::to_vec_pretty(value).context("serialize review cache entry")?;
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, bytes).with_context(|| format!("write {}", tmp.display()))?;
    fs::rename(&tmp, &path).with_context(|| format!("rename {}", tmp.display()))?;
    Ok(())
}

/// Absolute cache entry path for `key`.
fn entry_path(root: &Path, key: &CacheKey) -> Result<PathBuf> {
    let dir = git_dir(root)?.join("do-harness").join("pr-review");
    Ok(dir.join(format!(
        "v{}-{}-{}-{}.json",
        key.schema_version, key.merge_base, key.head_sha, key.policy_sha256
    )))
}

/// Absolute git directory of the repository containing `root`.
fn git_dir(root: &Path) -> Result<PathBuf> {
    let output = git_command(root)
        .args(["rev-parse", "--absolute-git-dir"])
        .output()
        .context("failed to run git rev-parse --absolute-git-dir")?;
    if !output.status.success() {
        bail!("not a git repository: {}", root.display());
    }
    let path = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if path.is_empty() {
        bail!("git rev-parse --absolute-git-dir returned no path");
    }
    Ok(PathBuf::from(path))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    fn key(head: &str) -> CacheKey {
        CacheKey {
            schema_version: 1,
            merge_base: "mb".to_owned(),
            head_sha: head.to_owned(),
            policy_sha256: "none".to_owned(),
        }
    }

    fn init_repo(dir: &Path) {
        let status = git_command(dir)
            .args(["init", "-q"])
            .status()
            .expect("spawn git");
        assert!(status.success());
    }

    #[test]
    fn round_trips_under_the_git_dir() {
        let dir = tempfile::tempdir().unwrap();
        init_repo(dir.path());
        let value = serde_json::json!({ "answer": 42 });
        store(dir.path(), &key("abc"), &value).expect("store");
        let loaded: serde_json::Value = load(dir.path(), &key("abc")).expect("hit");
        assert_eq!(loaded["answer"], serde_json::json!(42));
    }

    #[test]
    fn different_key_is_a_miss() {
        let dir = tempfile::tempdir().unwrap();
        init_repo(dir.path());
        store(dir.path(), &key("abc"), &serde_json::json!({})).expect("store");
        assert!(load::<serde_json::Value>(dir.path(), &key("def")).is_none());
    }

    #[test]
    fn non_repository_is_a_miss_and_store_error() {
        let dir = tempfile::tempdir().unwrap();
        assert!(load::<serde_json::Value>(dir.path(), &key("abc")).is_none());
        assert!(store(dir.path(), &key("abc"), &serde_json::json!({})).is_err());
    }

    #[test]
    fn stale_scratch_file_does_not_poison_the_hit() {
        let dir = tempfile::tempdir().unwrap();
        init_repo(dir.path());
        let path = entry_path(dir.path(), &key("abc")).unwrap();
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, b"not json").unwrap();
        assert!(load::<serde_json::Value>(dir.path(), &key("abc")).is_none());
    }
}
