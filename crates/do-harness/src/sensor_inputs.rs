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
use sha2::{Digest, Sha256};

use crate::config::SensorSpec;

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
        let path = std::str::from_utf8(raw).ok()?.to_owned();
        if Path::new(&path).is_absolute() {
            return None;
        }
        paths.push(path);
    }
    Some(paths)
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
    if !metadata.file_type().is_file() {
        return None;
    }
    let bytes = std::fs::read(&full).ok()?;
    Some((
        kind.to_owned(),
        path.to_owned(),
        hex::encode(Sha256::digest(&bytes)),
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
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    fn spec(inputs: &[&str]) -> SensorSpec {
        SensorSpec {
            kind: None,
            fix: None,
            name: "probe".to_owned(),
            argv: vec!["true".to_owned()],
            retry: None,
            timeout: None,
            severity: None,
            allow_failure: false,
            transient_exit_codes: Vec::new(),
            when_changed: Vec::new(),
            artifacts: Vec::new(),
            coverage_inputs: Vec::new(),
            inputs: inputs.iter().map(|i| (*i).to_owned()).collect(),
        }
    }

    fn git(root: &Path, args: &[&str]) {
        let status = crate::changes::git_command(root)
            .args(args)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_AUTHOR_NAME", "t")
            .env("GIT_AUTHOR_EMAIL", "t@t")
            .env("GIT_COMMITTER_NAME", "t")
            .env("GIT_COMMITTER_EMAIL", "t@t")
            .status()
            .expect("spawn git");
        assert!(status.success(), "git {args:?} failed");
    }

    /// Content edits, additions, and deletions of declared inputs change the
    /// identity; unrelated edits and edits to gitignored outputs do not.
    #[test]
    fn identity_tracks_declared_inputs_only() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::write(root.join("input.txt"), "v1\n").unwrap();
        std::fs::write(root.join("other.txt"), "v1\n").unwrap();
        git(root, &["init", "-q"]);
        git(root, &["add", "-A"]);
        git(root, &["commit", "-qm", "base"]);

        let probe = spec(&["input.txt"]);
        let first = digest(root, &probe, None).expect("git repo with a match");
        assert_eq!(digest(root, &probe, None).as_deref(), Some(first.as_str()));

        // Uncommitted, nonmatching edit: same identity.
        std::fs::write(root.join("other.txt"), "v2\n").unwrap();
        assert_eq!(digest(root, &probe, None).as_deref(), Some(first.as_str()));

        // Ignored untracked file matching the glob: same identity.
        std::fs::write(root.join(".gitignore"), "input-*.tmp\n").unwrap();
        std::fs::write(root.join("input-ignored.tmp"), "x\n").unwrap();
        let ignored_spec = spec(&["input*.txt", "input-*.tmp"]);
        let with_ignored = digest(root, &ignored_spec, None).unwrap();
        std::fs::write(root.join("input-ignored.tmp"), "y\n").unwrap();
        assert_eq!(
            digest(root, &ignored_spec, None).as_deref(),
            Some(with_ignored.as_str())
        );

        // Matching content edit changes the identity.
        std::fs::write(root.join("input.txt"), "v2\n").unwrap();
        assert_ne!(digest(root, &probe, None).as_deref(), Some(first.as_str()));

        // Deleting a tracked input makes the identity unavailable (`None`):
        // the declared input set is no longer complete, so the sensor runs.
        std::fs::remove_file(root.join("input.txt")).unwrap();
        assert!(digest(root, &probe, None).is_none());
    }

    /// A new file matching the glob changes the identity (addition).
    #[test]
    fn added_matching_file_changes_identity() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::write(root.join("a.txt"), "a\n").unwrap();
        git(root, &["init", "-q"]);
        git(root, &["add", "-A"]);
        git(root, &["commit", "-qm", "base"]);
        let spec = spec(&["*.txt"]);
        let before = digest(root, &spec, None).unwrap();
        std::fs::write(root.join("b.txt"), "b\n").unwrap();
        assert_ne!(digest(root, &spec, None).as_deref(), Some(before.as_str()));
    }

    /// A new commit alone changes the identity: history-dependent outcomes
    /// must not be reused across commits.
    #[test]
    fn new_commit_changes_identity() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::write(root.join("input.txt"), "v1\n").unwrap();
        git(root, &["init", "-q"]);
        git(root, &["add", "-A"]);
        git(root, &["commit", "-qm", "base"]);
        let spec = spec(&["input.txt"]);
        let before = digest(root, &spec, None).unwrap();
        git(root, &["commit", "--allow-empty", "-qm", "empty"]);
        assert_ne!(digest(root, &spec, None).as_deref(), Some(before.as_str()));
    }

    /// Uncertainty is never a reuse: no declaration, no git repository, no
    /// match, and a symlinked match all yield `None`.
    #[test]
    fn uncertainty_yields_none() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::write(root.join("input.txt"), "v1\n").unwrap();

        assert!(digest(root, &spec(&[]), None).is_none(), "no declaration");
        assert!(
            digest(root, &spec(&["input.txt"]), None).is_none(),
            "not a git repository"
        );

        git(root, &["init", "-q"]);
        assert!(
            digest(root, &spec(&["missing.txt"]), None).is_none(),
            "unmatched input set"
        );

        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(root.join("input.txt"), root.join("link.txt")).unwrap();
            assert!(
                digest(root, &spec(&["link.txt"]), None).is_none(),
                "symlink match"
            );
        }
    }
}
