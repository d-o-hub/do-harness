//! Unit tests for the `sensor_inputs` module.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;

fn spec(inputs: &[&str]) -> SensorSpec {
    SensorSpec {
        kind: None,
        fix: None,
        name: "probe".to_owned(),
        argv: vec!["true".to_owned()],
        retry: None,
        timeout_justification: None,
        max_output_bytes: None,
        environment_allowlist: None,
        redact_env: Vec::new(),
        timeout: None,
        severity: None,
        allow_failure: false,
        transient_exit_codes: Vec::new(),
        when_changed: Vec::new(),
        artifacts: Vec::new(),
        environment_inputs: Vec::new(),
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
