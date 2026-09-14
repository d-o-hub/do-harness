//! Artifact glob resolution and SHA-256 digesting for evidence.

use std::path::{Path, PathBuf};

use globset::{Glob, GlobSet, GlobSetBuilder};
use sha2::{Digest, Sha256};

/// Maximum artifacts recorded per sensor, bounding evidence size.
pub const MAX_ARTIFACTS: usize = 200;

/// One digested artifact file, repository-relative.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EvidenceArtifact {
    /// Repository-relative path (`/`-separated).
    pub path: String,
    /// Hex SHA-256 of the file bytes, or `"unreadable"`.
    pub sha256: String,
}

/// Expands repository-relative globs to sorted, digested artifact files.
///
/// Globs that fail to compile are ignored here (config validation rejects
/// them at load time). Each glob walks only its literal prefix, so broad
/// patterns like `**/*.png` stay bounded to the tree they need.
#[must_use]
pub fn resolve(root: &Path, patterns: &[String]) -> Vec<EvidenceArtifact> {
    let Some(set) = build_set(patterns) else {
        return Vec::new();
    };
    let mut artifacts = Vec::new();
    for prefix in prefixes(patterns) {
        let start = if prefix.as_os_str().is_empty() {
            root.to_path_buf()
        } else {
            root.join(prefix)
        };
        walk(root, &start, &set, &mut artifacts);
    }
    artifacts.sort_by(|a, b| a.path.cmp(&b.path));
    artifacts.dedup_by(|a, b| a.path == b.path);
    artifacts.truncate(MAX_ARTIFACTS);
    artifacts
}

/// Compiles `patterns` into a glob set; `None` when none compile.
fn build_set(patterns: &[String]) -> Option<GlobSet> {
    let mut builder = GlobSetBuilder::new();
    let mut any = false;
    for pattern in patterns {
        if let Ok(glob) = Glob::new(pattern) {
            builder.add(glob);
            any = true;
        }
    }
    if !any {
        return None;
    }
    builder.build().ok()
}

/// Literal directory prefixes of `patterns`, deduplicated.
fn prefixes(patterns: &[String]) -> Vec<PathBuf> {
    let mut prefixes: Vec<PathBuf> = patterns
        .iter()
        .map(|pattern| PathBuf::from(literal_prefix(pattern)))
        .collect();
    prefixes.sort();
    prefixes.dedup();
    prefixes
}

/// The directory portion of a glob before its first wildcard character.
fn literal_prefix(pattern: &str) -> String {
    let wildcard = pattern
        .find(['*', '?', '[', '{', '!'])
        .unwrap_or(pattern.len());
    let head = &pattern[..wildcard];
    match head.rfind('/') {
        Some(index) => head[..index].to_owned(),
        None => String::new(),
    }
}

/// Recursively collects files under `start` matching `set`, relative to `root`.
fn walk(root: &Path, start: &Path, set: &GlobSet, out: &mut Vec<EvidenceArtifact>) {
    let Ok(entries) = std::fs::read_dir(start) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_dir() {
            if entry.file_name() == ".git" {
                continue;
            }
            walk(root, &path, set, out);
            continue;
        }
        if !file_type.is_file() {
            continue;
        }
        let Ok(relative) = path.strip_prefix(root) else {
            continue;
        };
        let relative = relative.to_string_lossy().replace('\\', "/");
        if !set.is_match(relative.as_str()) {
            continue;
        }
        out.push(EvidenceArtifact {
            sha256: file_digest(&path),
            path: relative,
        });
    }
}

/// Hex SHA-256 of a file, or `"unreadable"` when it cannot be read.
fn file_digest(path: &Path) -> String {
    match std::fs::read(path) {
        Ok(bytes) => hex::encode(Sha256::digest(&bytes)),
        Err(_) => "unreadable".to_owned(),
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    /// Globs resolve recursively to sorted digests; unrelated files stay out.
    #[test]
    fn resolves_globs_to_sorted_digests() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("out/nested")).unwrap();
        std::fs::write(dir.path().join("out/b.png"), b"bee").unwrap();
        std::fs::write(dir.path().join("out/nested/a.png"), b"ay").unwrap();
        std::fs::write(dir.path().join("out/keep.txt"), b"no").unwrap();

        let artifacts = resolve(dir.path(), &["out/**/*.png".to_owned()]);
        let paths: Vec<&str> = artifacts.iter().map(|a| a.path.as_str()).collect();
        assert_eq!(paths, vec!["out/b.png", "out/nested/a.png"]);
        assert_eq!(
            artifacts[0].sha256,
            hex::encode(Sha256::digest(b"bee")),
            "digest must hash file bytes"
        );
    }

    /// A root-level glob with no literal prefix resolves from the root.
    #[test]
    fn root_level_glob_resolves() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("report.json"), b"{}").unwrap();
        let artifacts = resolve(dir.path(), &["**/*.json".to_owned()]);
        assert_eq!(artifacts.len(), 1);
        assert_eq!(artifacts[0].path, "report.json");
    }

    /// Invalid globs resolve to nothing instead of panicking.
    #[test]
    fn invalid_glob_is_ignored() {
        let dir = tempfile::tempdir().unwrap();
        assert!(resolve(dir.path(), &["[unclosed".to_owned()]).is_empty());
    }
}
