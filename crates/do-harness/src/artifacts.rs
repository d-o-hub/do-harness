//! Artifact glob resolution and SHA-256 digesting for evidence.

use std::io::Read;
use std::path::{Path, PathBuf};

use globset::{Glob, GlobSet, GlobSetBuilder};
use sha2::{Digest, Sha256};

/// Maximum artifacts recorded per sensor, bounding evidence size.
pub const MAX_ARTIFACTS: usize = 200;
const MAX_VISITS: usize = 10_000;
const MAX_DEPTH: usize = 64;
const MAX_ARTIFACT_BYTES: u64 = 64 * 1024 * 1024;
const DIGEST_BUFFER: usize = 64 * 1024;

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
    let Ok(root) = root.canonicalize() else {
        return Vec::new();
    };
    let Some(set) = build_set(patterns) else {
        return Vec::new();
    };
    let mut artifacts = Vec::new();
    let mut visits = 0;
    for prefix in prefixes(patterns) {
        let start = if prefix.as_os_str().is_empty() {
            root.clone()
        } else {
            root.join(prefix)
        };
        walk(&root, &start, &set, &mut artifacts, &mut visits, 0);
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
fn unverified(out: &mut Vec<EvidenceArtifact>) {
    if out.iter().any(|a| a.path == "@unverified") {
        return;
    }
    if out.len() >= MAX_ARTIFACTS {
        out.pop();
    }
    out.push(EvidenceArtifact {
        path: "@unverified".into(),
        sha256: "unreadable".into(),
    });
}

fn walk(
    root: &Path,
    start: &Path,
    set: &GlobSet,
    out: &mut Vec<EvidenceArtifact>,
    visits: &mut usize,
    depth: usize,
) {
    if out.len() >= MAX_ARTIFACTS || *visits >= MAX_VISITS || depth > MAX_DEPTH {
        unverified(out);
        return;
    }
    let Ok(start) = crate::managed_path::within(root, start) else {
        unverified(out);
        return;
    };
    let entries = match std::fs::read_dir(start) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return,
        Err(_) => {
            unverified(out);
            return;
        }
    };
    for entry in entries {
        *visits += 1;
        if out.len() >= MAX_ARTIFACTS || *visits > MAX_VISITS {
            unverified(out);
            return;
        }
        let Ok(entry) = entry else {
            unverified(out);
            continue;
        };
        let path = entry.path();
        let Ok(file_type) = entry.file_type() else {
            unverified(out);
            continue;
        };
        if file_type.is_dir() {
            if entry.file_name() != ".git" {
                walk(root, &path, set, out, visits, depth + 1);
            }
            continue;
        }
        let Ok(relative) = path.strip_prefix(root) else {
            unverified(out);
            continue;
        };
        let Some(relative) = relative.to_str() else {
            unverified(out);
            continue;
        };
        let relative = if cfg!(windows) {
            relative.replace('\\', "/")
        } else {
            relative.to_owned()
        };
        if !set.is_match(relative.as_str()) {
            continue;
        }
        if !file_type.is_file() || crate::managed_path::within(root, &path).is_err() {
            unverified(out);
            continue;
        }
        out.push(EvidenceArtifact {
            sha256: file_digest(&path),
            path: relative,
        });
    }
}

/// Stream bounded regular files; oversized/unreadable matches cannot prove a pass.
fn file_digest(path: &Path) -> String {
    let Ok(mut file) = std::fs::File::open(path) else {
        return "unreadable".into();
    };
    if !file
        .metadata()
        .is_ok_and(|m| m.is_file() && m.len() <= MAX_ARTIFACT_BYTES)
    {
        return "unreadable".into();
    }
    let mut hasher = Sha256::new();
    let mut buffer = vec![0; DIGEST_BUFFER];
    let mut total = 0;
    loop {
        match file.read(&mut buffer) {
            Ok(0) => break,
            Ok(count) => {
                total += u64::try_from(count).unwrap_or(u64::MAX);
                if total > MAX_ARTIFACT_BYTES {
                    return "unreadable".into();
                }
                hasher.update(&buffer[..count]);
            }
            Err(_) => return "unreadable".into(),
        }
    }
    hex::encode(hasher.finalize())
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
        assert_eq!(resolve(dir.path(), &["[unclosed".to_owned()]).len(), 0);
    }

    #[cfg(unix)]
    #[test]
    fn literal_prefix_symlinks_never_digest_outside_files() {
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        std::fs::write(outside.path().join("report.json"), "outside").unwrap();
        std::os::unix::fs::symlink(outside.path(), root.path().join("out")).unwrap();
        let artifacts = resolve(root.path(), &["out/*.json".into()]);
        assert!(artifacts.iter().any(|a| a.sha256 == "unreadable"));
        assert!(
            !artifacts
                .iter()
                .any(|a| a.sha256 == hex::encode(Sha256::digest(b"outside")))
        );
    }

    #[cfg(unix)]
    #[test]
    fn non_utf8_names_cannot_alias_a_verified_artifact() {
        use std::os::unix::ffi::OsStringExt;
        const INVALID_UTF8_BYTES: [u8; 2] = [0x80, 0x81];
        let root = tempfile::tempdir().unwrap();
        for byte in INVALID_UTF8_BYTES {
            let name = std::ffi::OsString::from_vec(vec![byte, b'.', b'j', b's', b'o', b'n']);
            std::fs::write(root.path().join(name), "{}").unwrap();
        }
        let artifacts = resolve(root.path(), &["*.json".into()]);
        assert!(artifacts.iter().all(|a| a.sha256 == "unreadable"));
        assert_eq!(artifacts.len(), 1);
    }

    #[test]
    fn oversized_artifacts_are_unverified_without_reading_them_into_memory() {
        let root = tempfile::tempdir().unwrap();
        let file = std::fs::File::create(root.path().join("large.bin")).unwrap();
        file.set_len(MAX_ARTIFACT_BYTES + 1).unwrap();
        let artifacts = resolve(root.path(), &["*.bin".into()]);
        assert_eq!(artifacts[0].sha256, "unreadable");
    }
}
