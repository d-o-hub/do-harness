//! Commitlint gate fixtures: the subject window must judge this branch.
//!
//! The `commitlint` sensor is one of the few whose *input* is history rather
//! than the working tree, which gives its window its own failure modes: it can
//! descend into the base branch it merged, and it can lint nothing at all while
//! still exiting 0. Both are pinned here.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::{Path, PathBuf};
use std::process::Command;

/// Builds a command with hook-inherited repository environment removed.
fn isolated_command(program: &str) -> Command {
    let mut command = Command::new(program);
    for key in [
        "GIT_DIR",
        "GIT_WORK_TREE",
        "GIT_INDEX_FILE",
        "GIT_OBJECT_DIRECTORY",
        "GIT_ALTERNATE_OBJECT_DIRECTORIES",
        "GIT_CEILING_DIRECTORIES",
        "GIT_NAMESPACE",
        "GIT_PREFIX",
    ] {
        command.env_remove(key);
    }
    command
}

/// The canonical script, with the shipped template mirror asserted identical.
fn script_source() -> &'static str {
    let source = include_str!("../../../scripts/check-commitlint.sh");
    assert_eq!(
        source,
        include_str!("../templates/scripts/check-commitlint.sh"),
        "the shipped template must not drift from scripts/"
    );
    source
}

/// Creates a fixture repo carrying the script, returning its root.
fn fixture() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_path_buf();
    std::fs::create_dir(root.join("scripts")).unwrap();
    std::fs::write(root.join("scripts/check-commitlint.sh"), script_source()).unwrap();
    (dir, root)
}

/// Runs the script in `root`, returning (success, stdout, stderr).
fn lint(root: &Path, args: &[&str]) -> (bool, String, String) {
    let output = isolated_command("bash")
        .arg(root.join("scripts/check-commitlint.sh"))
        .args(args)
        .env_remove("DO_HARNESS_COMMITLINT_COUNT")
        .current_dir(root)
        .output()
        .expect("spawn check-commitlint.sh");
    (
        output.status.success(),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

/// Runs `git` in `root`, optionally pinning author and committer dates.
fn git(root: &Path, args: &[&str], date: Option<&str>) {
    let mut command = isolated_command("git");
    command
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_AUTHOR_NAME", "t")
        .env("GIT_AUTHOR_EMAIL", "t@t")
        .env("GIT_COMMITTER_NAME", "t")
        .env("GIT_COMMITTER_EMAIL", "t@t")
        .args(args)
        .current_dir(root);
    if let Some(date) = date {
        command
            .env("GIT_AUTHOR_DATE", date)
            .env("GIT_COMMITTER_DATE", date);
    }
    let output = command.output().expect("spawn git");
    assert!(
        output.status.success(),
        "git {args:?} failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// Commits an empty change with a pinned date.
fn commit(root: &Path, subject: &str, date: &str) {
    git(
        root,
        &["commit", "-q", "--allow-empty", "-m", subject],
        Some(date),
    );
}

#[test]
fn commitlint_empty_history_validates_arguments_and_messages() {
    let (_dir, root) = fixture();
    git(&root, &["init", "-q"], None);
    std::fs::write(root.join("valid-message"), "fix: initial commit\n").unwrap();
    std::fs::write(root.join("invalid-message"), "invalid subject\n").unwrap();

    let cases: &[(&[&str], i32)] = &[
        (&[], 0),
        (&["--count", "2"], 0),
        (&["--count", "0"], 2),
        (&["--unknown"], 2),
        (&["--range", "missing..HEAD"], 2),
        (&["--range=missing..HEAD"], 2),
        (&["--range"], 2),
        (&["--range", ""], 2),
        (&["--range="], 2),
        (&["--range", "missing..HEAD", "--count", "1"], 2),
        (&["--message", "valid-message"], 0),
        (&["--message", "invalid-message"], 1),
    ];
    for (args, expected) in cases {
        let output = isolated_command("bash")
            .arg(root.join("scripts/check-commitlint.sh"))
            .args(*args)
            .env_remove("DO_HARNESS_COMMITLINT_COUNT")
            .current_dir(&root)
            .output()
            .unwrap();
        assert_eq!(
            output.status.code(),
            Some(*expected),
            "args {args:?}: stdout={} stderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

/// A merge tip is judged on the branch's commits, not the base it merged.
///
/// Regression: the window ran `git log --no-merges -n 1`, which skips a merge
/// tip and descends into whatever it merged; `git log` orders by date, so the
/// window could land on a commit already on the base branch. Observed in CI as
/// a PR reporting the non-conventional subject of a commit on `main`.
///
/// A first attempt fixed that with `--first-parent`, which is wrong in general:
/// a merge created from the feature branch has the branch as parent 1, but
/// `GitHub`'s test-merge (below) has the base as parent 1. `--first-parent`
/// therefore fixed one shape and broke the other. Excluding the base ref
/// expresses the intent without depending on parent order.
#[test]
fn commitlint_lints_the_branch_not_the_base_it_merged() {
    let (_dir, root) = fixture();
    git(&root, &["init", "-q", "-b", "main"], None);
    commit(&root, "chore: seed", "1700000000");

    git(&root, &["checkout", "-q", "-b", "feature"], None);
    commit(&root, "feat(feature): a conventional change", "1700000100");

    git(&root, &["checkout", "-q", "main"], None);
    // Non-conventional on the base, and newer, so a naive window reaches it.
    commit(&root, "Fix something without a type", "1700000200");
    git(
        &root,
        &["update-ref", "refs/remotes/origin/main", "HEAD"],
        None,
    );

    git(&root, &["checkout", "-q", "feature"], None);
    git(
        &root,
        &[
            "merge",
            "-q",
            "--no-ff",
            "-m",
            "Merge branch 'main' into feature",
            "main",
        ],
        Some("1700000300"),
    );

    let (ok, stdout, stderr) = lint(&root, &[]);
    assert!(
        ok,
        "a merge tip must not fail on the base history it merged:\n{stdout}\n{stderr}"
    );
    assert!(
        !stdout.contains("Fix something without a type"),
        "the base commit's subject must not be linted:\n{stdout}"
    );
    assert!(
        stdout.contains("not on origin/main"),
        "the window must be scoped to commits the branch adds:\n{stdout}"
    );

    // The branch's own bad subject must still fail, so the window is not vacuous.
    commit(&root, "Bad subject on the branch", "1700000400");
    let (ok, stdout, _) = lint(&root, &[]);
    assert!(
        !ok,
        "the branch's own non-conventional subject must still fail:\n{stdout}"
    );

    // With no base ref the window must still lint the tip, never nothing.
    git(
        &root,
        &["update-ref", "-d", "refs/remotes/origin/main"],
        None,
    );
    commit(&root, "feat(feature): conventional again", "1700000500");
    let (ok, stdout, stderr) = lint(&root, &[]);
    assert!(
        ok,
        "no base ref must fall back to the plain window:\n{stderr}"
    );
    assert!(
        stdout.contains("1 subject(s)"),
        "the fallback must lint the tip, not nothing:\n{stdout}"
    );
}

/// The window is correct in `GitHub`'s test-merge shape, where the base is
/// parent 1.
///
/// `GitHub` builds `refs/pull/<n>/merge` from the **base branch**, so the base
/// is parent 1 and the PR branch is parent 2 — the opposite of a merge created
/// from the feature branch. Both `--no-merges -n 1` and
/// `--first-parent --no-merges -n 1` land on the base's commit here, which is
/// exactly how a `--first-parent` "fix" passed the other fixture and still
/// failed in CI. Scoping by excluding the base ref is correct in both shapes.
#[test]
fn commitlint_excludes_the_base_in_github_test_merge_shape() {
    let (_dir, root) = fixture();
    git(&root, &["init", "-q", "-b", "main"], None);
    commit(&root, "chore: seed", "1700000000");
    git(&root, &["checkout", "-q", "-b", "feature"], None);
    commit(&root, "feat(feature): a conventional change", "1700000100");
    git(&root, &["checkout", "-q", "main"], None);
    commit(&root, "Fix something without a type", "1700000200");
    git(
        &root,
        &["update-ref", "refs/remotes/origin/main", "HEAD"],
        None,
    );

    // Build the merge from the base side, as `GitHub` does.
    git(
        &root,
        &[
            "merge",
            "-q",
            "--no-ff",
            "-m",
            "Merge pull request",
            "feature",
        ],
        Some("1700000300"),
    );
    let parents = isolated_command("git")
        .args(["log", "-1", "--pretty=%p"])
        .current_dir(&root)
        .output()
        .unwrap();
    assert_eq!(
        String::from_utf8_lossy(&parents.stdout)
            .split_whitespace()
            .count(),
        2,
        "the fixture must produce a real merge commit"
    );

    let (ok, stdout, stderr) = lint(&root, &[]);
    assert!(
        ok,
        "the base's own subject must not fail the gate:\n{stdout}\n{stderr}"
    );
    assert!(
        !stdout.contains("Fix something without a type"),
        "the base commit must not be linted:\n{stdout}"
    );
    assert!(
        stdout.contains("not on origin/main"),
        "the window must be scoped to the branch's commits:\n{stdout}"
    );
}
