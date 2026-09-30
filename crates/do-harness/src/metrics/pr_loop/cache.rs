//! Snapshot cache for the PR-loop measures.
//!
//! The cache lives under the repository git directory (never in the work tree)
//! and never affects a measure's meaning: a missing, unreadable, expired, or
//! schema-mismatched entry simply forces a refetch. The snapshot records when it
//! was fetched, and the report prints that stamp, so a cached run is auditable
//! rather than silently stale.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use super::PrLoopSnapshot;
use super::window::now_epoch;
use crate::pr::cache;

/// Cached snapshots older than this are refetched even without `--recompute`.
pub(crate) const CACHE_TTL_SECS: i64 = 600;

/// Cache file for one (repository, window, limit) triple.
fn cache_path(root: &Path, repo: &str, since: &str, limit: usize) -> Result<PathBuf> {
    let dir = cache::git_dir(root)?.join("do-harness").join("pr-metrics");
    let slug = repo.replace('/', "-");
    Ok(dir.join(format!(
        "v{}-{slug}-{}-{limit}.json",
        super::SCHEMA_VERSION,
        since.replace('/', "-")
    )))
}

/// Reads a fresh cached snapshot, when one exists.
pub(crate) fn cached(root: &Path, repo: &str, since: &str, limit: usize) -> Option<PrLoopSnapshot> {
    let path = cache_path(root, repo, since, limit).ok()?;
    let bytes = std::fs::read(path).ok()?;
    let snapshot: PrLoopSnapshot = serde_json::from_slice(&bytes).ok()?;
    let age = now_epoch() - snapshot.fetched_at;
    (snapshot.schema_version == super::SCHEMA_VERSION && (0..=CACHE_TTL_SECS).contains(&age))
        .then_some(snapshot)
}

/// Writes the snapshot, atomically.
pub(crate) fn store(root: &Path, snapshot: &PrLoopSnapshot) -> Result<()> {
    let path = cache_path(root, &snapshot.repo, &snapshot.since, snapshot.limit)?;
    let dir = path.parent().context("cache path has no parent")?;
    std::fs::create_dir_all(dir).with_context(|| format!("create {}", dir.display()))?;
    let bytes = serde_json::to_vec_pretty(snapshot).context("serialize pr metrics snapshot")?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, bytes).with_context(|| format!("write {}", tmp.display()))?;
    std::fs::rename(&tmp, &path).with_context(|| format!("rename {}", tmp.display()))?;
    Ok(())
}
