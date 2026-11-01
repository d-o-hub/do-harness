//! Exact input identity for a sensor, for unchanged-input reuse.
//!
//! A sensor may declare `inputs = [...]` globs naming every repository file
//! whose content can change its outcome. [`digest`] hashes an identity over
//! those files; `verify --record` stores it on a clean passing beat and
//! later runs compare it to decide whether the recorded pass still
//! describes the same inputs.
//!
//! Every uncertainty yields `None`, which means "run": an empty declaration,
//! git unavailable, a non-UTF8 or missing path, a symlink, an unreadable
//! file, or an input set matching nothing at all. The cache is never allowed
//! to turn missing information into a reused pass. Commands that read the
//! environment, network, generated outputs, or history cannot declare a
//! complete input set and must stay undeclared.

use std::path::Path;

use globset::{GlobBuilder, GlobSet, GlobSetBuilder};

use crate::config::SensorSpec;

/// Computes a conservative input identity for a sensor that has no declared
/// `inputs`.
///
/// This covers all repository source paths (from index and untracked files)
/// excluding harness-owned state (`.do-harness/`), gitignored files, and
/// untracked declared output artifacts (`artifacts` globs). If any untracked
/// declared artifact glob matches an untracked file, that untracked output is
/// excluded so generated output creation during run doesn't invalidate. Tracked
/// output files remain included so git-managed source files are never hidden.
#[must_use]
pub(crate) fn workspace_source_digest(
    root: &Path,
    spec: &SensorSpec,
    config_bytes: Option<&[u8]>,
) -> Option<String> {
    let paths = repository_paths(root)?;
    let artifact_set = if spec.artifacts.is_empty() {
        None
    } else {
        compile(&spec.artifacts)
    };
    let coverage_set = if spec.coverage_inputs.is_empty() {
        None
    } else {
        compile(&spec.coverage_inputs)
    };

    let untracked = untracked_paths(root).unwrap_or_default();

    let mut entries: Vec<Entry> = Vec::new();
    for path in &paths {
        if crate::fingerprint::is_harness_state(path) {
            continue;
        }
        let is_untracked = untracked.contains(path);
        if is_untracked
            && (path == "Cargo.lock"
                || artifact_set
                    .as_ref()
                    .is_some_and(|set| set.is_match(path.as_str())))
        {
            continue;
        }
        entries.push(entry(root, path, "source")?);
        if coverage_set
            .as_ref()
            .is_some_and(|set| set.is_match(path.as_str()))
        {
            entries.push(entry(root, path, "coverage")?);
        }
    }

    entries.sort();
    entries.dedup();
    let files: Vec<serde_json::Value> = entries
        .iter()
        .map(|(kind, path, sha256, mode)| {
            serde_json::json!({ "kind": kind, "path": path, "sha256": sha256, "mode": mode })
        })
        .collect();
    let payload = serde_json::json!({
        "sensor": serde_json::to_value(spec).ok()?,
        "config": crate::fingerprint::config_digest(config_bytes),
        "harness_version": env!("CARGO_PKG_VERSION"),
        "baselines": crate::baselines::digest(root),
        "head": head_sha(root),
        "files": files,
    });
    Some(crate::fingerprint::hash_canonical(&payload))
}

/// Returns input identity for `spec`: declared input digest if declared, or
/// conservative workspace source digest if undeclared.
#[must_use]
pub(crate) fn effective_input_digest(
    root: &Path,
    spec: &SensorSpec,
    config_bytes: Option<&[u8]>,
) -> Option<String> {
    if spec.inputs.is_empty() {
        workspace_source_digest(root, spec, config_bytes)
    } else {
        digest(root, spec, config_bytes)
    }
}

/// Computes the identity of `spec`'s declared inputs, or `None` when the
/// sensor must run.
///
/// The identity covers, in one canonical hash:
/// - every repository-index and nonignored-untracked path matching `inputs`
///   (current bytes and file mode) and `coverage-inputs`;
/// - the complete sensor spec (so an `argv`/globs/severity edit changes it);
/// - the raw config bytes (or the built-in marker), harness version, and
///   blessed-baseline digest, matching the policy fingerprint inputs;
/// - the current `HEAD` commit, when one exists, so history-dependent
///   outcomes cannot be reused across commits.
#[must_use]
pub(crate) fn digest(
    root: &Path,
    spec: &SensorSpec,
    config_bytes: Option<&[u8]>,
) -> Option<String> {
    if spec.inputs.is_empty() {
        return None;
    }
    let paths = repository_paths(root)?;
    let inputs = compile(&spec.inputs)?;
    let coverage = if spec.coverage_inputs.is_empty() {
        None
    } else {
        Some(compile(&spec.coverage_inputs)?)
    };
    let mut entries: Vec<Entry> = Vec::new();
    let mut matched_input = false;
    for path in &paths {
        if inputs.is_match(path.as_str()) {
            matched_input = true;
            entries.push(entry(root, path, "input")?);
        }
        if coverage
            .as_ref()
            .is_some_and(|set| set.is_match(path.as_str()))
        {
            entries.push(entry(root, path, "coverage")?);
        }
    }
    if !matched_input {
        return None;
    }
    entries.sort();
    entries.dedup();
    let files: Vec<serde_json::Value> = entries
        .iter()
        .map(|(kind, path, sha256, mode)| {
            serde_json::json!({ "kind": kind, "path": path, "sha256": sha256, "mode": mode })
        })
        .collect();
    let payload = serde_json::json!({
        "sensor": serde_json::to_value(spec).ok()?,
        "config": crate::fingerprint::config_digest(config_bytes),
        "harness_version": env!("CARGO_PKG_VERSION"),
        "baselines": crate::baselines::digest(root),
        "head": head_sha(root),
        "files": files,
    });
    Some(crate::fingerprint::hash_canonical(&payload))
}

/// One hashed input file: kind (`input`/`coverage`), repository-relative
/// path, hex content digest, and file mode.
type Entry = (String, String, String, u32);

/// Repository-relative paths from the index plus nonignored untracked files,
/// or `None` when git cannot enumerate them or a path is not UTF-8.
fn repository_paths(root: &Path) -> Option<Vec<String>> {
    let output = crate::changes::git_command(root)
        .args([
            "ls-files",
            "--cached",
            "--others",
            "--exclude-standard",
            "-z",
        ])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let mut paths = Vec::new();
    for raw in output.stdout.split(|byte| *byte == 0) {
        if raw.is_empty() {
            continue;
        }
        let raw_str = std::str::from_utf8(raw).ok()?;
        if Path::new(raw_str).is_absolute() {
            return None;
        }
        paths.push(crate::changes::normalize(raw_str));
    }
    Some(paths)
}

/// Repository-relative nonignored untracked paths, or `None` when git cannot
/// enumerate them or a path is not UTF-8.
fn untracked_paths(root: &Path) -> Option<std::collections::HashSet<String>> {
    let output = crate::changes::git_command(root)
        .args(["ls-files", "--others", "--exclude-standard", "-z"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let mut set = std::collections::HashSet::new();
    for raw in output.stdout.split(|byte| *byte == 0) {
        if raw.is_empty() {
            continue;
        }
        let raw_str = std::str::from_utf8(raw).ok()?;
        if !Path::new(raw_str).is_absolute() {
            set.insert(crate::changes::normalize(raw_str));
        }
    }
    Some(set)
}

/// Compiles declared globs with the same `literal_separator` policy as
/// config validation; `None` when a pattern does not compile.
fn compile(patterns: &[String]) -> Option<GlobSet> {
    let mut builder = GlobSetBuilder::new();
    for pattern in patterns {
        let glob = GlobBuilder::new(pattern)
            .literal_separator(true)
            .build()
            .ok()?;
        builder.add(glob);
    }
    builder.build().ok()
}

/// Hashes one matched file; `None` when it is not a regular readable file
/// (missing, unreadable, a symlink, or another nonregular kind).
fn entry(root: &Path, path: &str, kind: &str) -> Option<Entry> {
    let full = root.join(path);
    let metadata = std::fs::symlink_metadata(&full).ok()?;
    if kind == "source" && metadata.file_type().is_symlink() {
        let target = full.canonicalize().ok()?;
        let canonical_root = root.canonicalize().ok()?;
        let relative = target.strip_prefix(&canonical_root).ok()?.to_str()?;
        let content = if target.is_dir() {
            // Git records the link itself. The target tree's tracked and
            // nonignored files already enter the source manifest separately.
            "directory".to_owned()
        } else {
            crate::fingerprint::content_hash(&target)?
        };
        let digest = crate::fingerprint::hash_canonical(&serde_json::json!({
            "target": relative, "content": content,
        }));
        return Some((
            kind.to_owned(),
            path.to_owned(),
            digest,
            file_mode(&metadata),
        ));
    }
    if !metadata.file_type().is_file() {
        return None;
    }
    if !full
        .canonicalize()
        .ok()?
        .starts_with(root.canonicalize().ok()?)
    {
        return None;
    }
    Some((
        kind.to_owned(),
        path.to_owned(),
        crate::fingerprint::content_hash(&full)?,
        file_mode(&metadata),
    ))
}

/// Permission bits of a file. Unix hashes the full mode; other platforms
/// hash the readonly flag, which is the only portable distinction.
#[cfg(unix)]
fn file_mode(metadata: &std::fs::Metadata) -> u32 {
    use std::os::unix::fs::PermissionsExt;
    metadata.permissions().mode()
}

/// Permission identity on non-Unix platforms.
#[cfg(not(unix))]
fn file_mode(metadata: &std::fs::Metadata) -> u32 {
    u32::from(metadata.permissions().readonly())
}

/// Current `HEAD` commit, when the repository has one.
fn head_sha(root: &Path) -> Option<String> {
    let output = crate::changes::git_command(root)
        .args(["rev-parse", "--verify", "HEAD"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let sha = String::from_utf8(output.stdout).ok()?.trim().to_owned();
    (!sha.is_empty()).then_some(sha)
}

#[cfg(test)]
#[path = "sensor_inputs_tests.rs"]
mod tests;
