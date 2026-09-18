//! Metadata cache for `skills suggest` at `.do-harness/cache/skills-v1.json`.
//!
//! The cache holds selection metadata only — never a skill body. It is a pure
//! optimization: an unreadable, corrupt, mis-versioned, or stale entry falls
//! back to a fresh frontmatter scan, and a cache write failure is a warning
//! rather than an error, so a read-only or full disk cannot break selection.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::catalog::SkillMetadata;

/// Cache location relative to the resolved workspace root.
pub const CACHE_PATH: &str = ".do-harness/cache/skills-v1.json";

/// Cache schema version; bumping it invalidates every entry.
pub const SCHEMA_VERSION: u32 = 1;

/// Scoring-version tag. The deterministic ranker's behavior is part of the
/// cache identity, so changing the algorithm invalidates cached entries rather
/// than silently mixing two scorings in one benchmark row.
pub const SCORING_VERSION: u32 = 1;

/// One cached skill's metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entry {
    /// Skill name from frontmatter.
    pub name: String,
    /// Skill description from frontmatter.
    pub description: String,
    /// `metadata.short-description`, when the frontmatter carried one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub short_description: Option<String>,
    /// sha256 of the frontmatter block this entry was parsed from.
    pub frontmatter_sha256: String,
}

/// On-disk cache envelope.
#[derive(Debug, Serialize, Deserialize)]
struct File {
    schema_version: u32,
    scoring_version: u32,
    /// Keyed by canonical `SKILL.md` path.
    entries: BTreeMap<String, Entry>,
}

/// Loaded cache contents, plus the entries the current scan refreshed.
#[derive(Debug, Default)]
pub struct Cache {
    loaded: BTreeMap<String, Entry>,
    fresh: BTreeMap<String, Entry>,
}

impl Cache {
    /// Loads the cache, warning and continuing empty when it is unusable.
    #[must_use]
    pub fn load(root: &Path, warnings: &mut Vec<String>) -> Cache {
        let path = root.join(CACHE_PATH);
        let Ok(text) = fs::read_to_string(&path) else {
            return Cache::default();
        };
        let Ok(file) = serde_json::from_str::<File>(&text) else {
            warnings.push(format!("ignoring unparsable skill cache {CACHE_PATH}"));
            return Cache::default();
        };
        if file.schema_version != SCHEMA_VERSION || file.scoring_version != SCORING_VERSION {
            let kind = if file.schema_version == SCHEMA_VERSION {
                "stale-scoring"
            } else {
                "stale-schema"
            };
            warnings.push(format!("ignoring {kind} skill cache at {CACHE_PATH}"));
            return Cache::default();
        }
        // Guard against a cache that claims to hold bodies: the selector may
        // only ever be served metadata, whatever is on disk.
        Cache {
            loaded: file.entries,
            fresh: BTreeMap::new(),
        }
    }

    /// Returns the cached metadata for `canonical`, when its frontmatter hash
    /// still matches the bytes on disk.
    #[must_use]
    pub fn lookup(&self, canonical: &Path, frontmatter_sha256: &str) -> Option<&Entry> {
        let entry = self.loaded.get(&key(canonical))?;
        (entry.frontmatter_sha256 == frontmatter_sha256).then_some(entry)
    }

    /// Records freshly parsed metadata for the next [`Cache::store`].
    pub fn remember(
        &mut self,
        canonical: &Path,
        name: &str,
        description: &str,
        short_description: Option<String>,
        frontmatter_sha256: &str,
    ) {
        self.fresh.insert(
            key(canonical),
            Entry {
                name: name.to_owned(),
                description: description.to_owned(),
                short_description,
                frontmatter_sha256: frontmatter_sha256.to_owned(),
            },
        );
    }

    /// Writes the refreshed cache; failures are warnings, never errors.
    ///
    /// Surviving entries are the union of the loaded cache and this scan's
    /// fresh parses, pruned to the skills still present in the catalog. Keeping
    /// the loaded entries matters: a scan where only one skill changed would
    /// otherwise rewrite the cache with that single entry and force every other
    /// skill to re-parse on the next run.
    pub fn store(mut self, root: &Path, skills: &[SkillMetadata], warnings: &mut Vec<String>) {
        let present: std::collections::BTreeSet<String> = skills
            .iter()
            .map(|skill| key(&skill.canonical_path))
            .collect();
        let mut entries = std::mem::take(&mut self.loaded);
        entries.extend(self.fresh);
        entries.retain(|path, _| present.contains(path));
        for skill in skills {
            if let Some(entry) = entries.get_mut(&key(&skill.canonical_path)) {
                entry.name.clone_from(&skill.name);
                entry.description.clone_from(&skill.description);
                entry.short_description.clone_from(&skill.short_description);
            }
        }
        if entries.is_empty() {
            return;
        }
        let path = root.join(CACHE_PATH);
        if let Some(parent) = path.parent() {
            if let Err(err) = fs::create_dir_all(parent) {
                warnings.push(format!("skill cache not written: {err}"));
                return;
            }
        }
        let file = File {
            schema_version: SCHEMA_VERSION,
            scoring_version: SCORING_VERSION,
            entries,
        };
        match serde_json::to_string_pretty(&file) {
            Ok(text) => {
                if let Err(err) = fs::write(&path, text) {
                    warnings.push(format!("skill cache not written: {err}"));
                }
            }
            Err(err) => warnings.push(format!("skill cache not written: {err}")),
        }
    }
}

/// Cache key for a canonical `SKILL.md` path.
fn key(canonical: &Path) -> String {
    canonical.to_string_lossy().replace('\\', "/")
}

/// Hex sha256 of `bytes`.
#[must_use]
pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}
