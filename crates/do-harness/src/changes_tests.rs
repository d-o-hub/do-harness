//! Unit tests for the `changes` module.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;

/// `normalize` unifies separators and strips leading `./`.
#[test]
fn normalize_unifies_separators() {
    assert_eq!(normalize("a/b"), "a/b");
    assert_eq!(
        normalize("a\\b"),
        if cfg!(windows) { "a/b" } else { "a\\b" }
    );
    assert_eq!(normalize("./a/b"), "a/b");
}

/// `GIT_VIEW_ENV` must cover what git itself calls the local environment.
///
/// `githooks(5)` documents clearing exactly `git rev-parse
/// --local-env-vars` before invoking git on another repository, so a
/// future git release that adds a variable to that list must fail here
/// instead of silently leaking the caller's repository.
#[test]
fn git_view_env_covers_gits_local_environment() {
    let output = Command::new("git")
        .args(["rev-parse", "--local-env-vars"])
        .output()
        .expect("spawn git");
    assert!(
        output.status.success(),
        "git rev-parse --local-env-vars failed"
    );
    let reported: Vec<String> = String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_owned)
        .collect();
    assert!(
        reported.len() >= GIT_VIEW_ENV.len() - 3,
        "git reported implausibly few local variables: {reported:?}"
    );
    for name in &reported {
        if GIT_VIEW_ENV_EXEMPT.contains(&name.as_str()) {
            assert!(
                !GIT_VIEW_ENV.contains(&name.as_str()),
                "{name} carries caller configuration and must stay inherited"
            );
            continue;
        }
        assert!(
            GIT_VIEW_ENV.contains(&name.as_str()),
            "GIT_VIEW_ENV must clear {name}"
        );
    }
}

/// Outside a git checkout discovery fails closed.
#[test]
fn discover_fails_closed_outside_git() {
    let dir = tempfile::tempdir().unwrap();
    let changed = discover(dir.path());
    assert!(changed.discovery_failed);
    assert_eq!(changed.files.len(), 0);
}

/// A committed repo reports staged, unstaged, deleted, and untracked files.
#[test]
fn discover_reports_all_change_kinds() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    git(root, &["init", "-q"]);
    std::fs::write(root.join("keep.rs"), "v1\n").unwrap();
    std::fs::write(root.join("drop.rs"), "gone\n").unwrap();
    std::fs::write(root.join("docs.md"), "docs\n").unwrap();
    git(root, &["add", "-A"]);
    git(root, &["commit", "-qm", "test: base"]);

    std::fs::write(root.join("keep.rs"), "v2\n").unwrap();
    std::fs::write(root.join("docs.md"), "docs v2\n").unwrap();
    std::fs::remove_file(root.join("drop.rs")).unwrap();
    std::fs::write(root.join("new.md"), "new\n").unwrap();

    let changed = discover(root);
    assert!(!changed.discovery_failed);
    assert_eq!(
        changed.files,
        vec![
            ChangedFile {
                path: "docs.md".to_owned(),
                kind: ChangeKind::Modified,
            },
            ChangedFile {
                path: "drop.rs".to_owned(),
                kind: ChangeKind::Deleted,
            },
            ChangedFile {
                path: "keep.rs".to_owned(),
                kind: ChangeKind::Modified,
            },
            ChangedFile {
                path: "new.md".to_owned(),
                kind: ChangeKind::Untracked,
            },
        ]
    );
}

/// A staged rename surfaces both ends: the new path with its origin and
/// the origin as a deletion, so `when-changed` globs over the source tree
/// still fire for a move out of it.
#[test]
fn discover_reports_renames() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    git(root, &["init", "-q"]);
    std::fs::write(root.join("old.rs"), "v1\n").unwrap();
    git(root, &["add", "-A"]);
    git(root, &["commit", "-qm", "test: base"]);
    git(root, &["mv", "old.rs", "new.rs"]);

    let changed = discover(root);
    assert!(!changed.discovery_failed);
    assert_eq!(
        changed.files,
        vec![
            ChangedFile {
                path: "new.rs".to_owned(),
                kind: ChangeKind::Renamed {
                    from: "old.rs".to_owned(),
                },
            },
            ChangedFile {
                path: "old.rs".to_owned(),
                kind: ChangeKind::Deleted,
            },
        ]
    );
}

/// Runs a git command inside `root`, asserting success.
fn git(root: &Path, args: &[&str]) {
    let status = git_command(root)
        .args(args)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_AUTHOR_NAME", "t")
        .env("GIT_AUTHOR_EMAIL", "t@t")
        .env("GIT_COMMITTER_NAME", "t")
        .env("GIT_COMMITTER_EMAIL", "t@t")
        .status()
        .unwrap();
    assert!(status.success());
}

/// Resolves `spec` to a full SHA inside `root`.
fn rev(root: &Path, spec: &str) -> String {
    let output = git_command(root)
        .args(["rev-parse", spec])
        .output()
        .unwrap();
    assert!(output.status.success(), "git rev-parse {spec} failed");
    String::from_utf8_lossy(&output.stdout).trim().to_owned()
}

/// A clean checkout of a committed branch still reports the committed range.
#[test]
fn discover_selection_reports_committed_range_on_clean_checkout() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    git(root, &["init", "-q"]);
    std::fs::write(root.join("main.rs"), "v1\n").unwrap();
    git(root, &["add", "-A"]);
    git(root, &["commit", "-qm", "test: base"]);
    let base = rev(root, "HEAD");
    std::fs::write(root.join("main.rs"), "v2\n").unwrap();
    std::fs::write(root.join("docs.md"), "docs\n").unwrap();
    git(root, &["add", "-A"]);
    git(root, &["commit", "-qm", "test: change"]);

    assert!(
        discover(root).files.is_empty(),
        "the checkout must be clean for this fixture"
    );

    let selection = discover_selection(root, false, Some(&base));
    assert_eq!(selection.mode, SelectionMode::Range);
    assert!(!selection.changed.discovery_failed);
    assert_eq!(selection.paths(), vec!["docs.md", "main.rs"]);
    let range = selection.range.expect("resolved range");
    assert_eq!(range.requested, base);
    assert_eq!(range.base_sha, base);
    assert_eq!(range.head_sha, rev(root, "HEAD"));
    assert_eq!(range.merge_base_sha, base);
}

/// `--changed` and `--since` union committed and uncommitted paths.
#[test]
fn discover_selection_unions_range_and_working_tree() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    git(root, &["init", "-q"]);
    std::fs::write(root.join("main.rs"), "v1\n").unwrap();
    git(root, &["add", "-A"]);
    git(root, &["commit", "-qm", "test: base"]);
    let base = rev(root, "HEAD");
    std::fs::write(root.join("main.rs"), "v2\n").unwrap();
    git(root, &["commit", "-qam", "test: committed"]);
    std::fs::write(root.join("notes.md"), "untracked\n").unwrap();

    let selection = discover_selection(root, true, Some(&base));
    assert_eq!(selection.mode, SelectionMode::WorkingTreeAndRange);
    assert!(!selection.changed.discovery_failed);
    assert_eq!(selection.paths(), vec!["main.rs", "notes.md"]);
}

/// An unresolvable base fails closed with a diagnostic instead of an empty set.
#[test]
fn discover_selection_fails_closed_for_missing_base() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    git(root, &["init", "-q"]);
    std::fs::write(root.join("main.rs"), "v1\n").unwrap();
    git(root, &["add", "-A"]);
    git(root, &["commit", "-qm", "test: base"]);

    let selection = discover_selection(root, false, Some("origin/does-not-exist"));
    assert!(selection.changed.discovery_failed);
    assert!(selection.range.is_none());
    assert!(
        selection
            .diagnostic
            .as_deref()
            .is_some_and(|d| d.contains("origin/does-not-exist")),
        "diagnostic must name the unresolvable ref: {:?}",
        selection.diagnostic
    );
}

/// A rename in the committed range reports both ends.
#[test]
fn discover_selection_reports_renames_in_range() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    git(root, &["init", "-q"]);
    std::fs::write(root.join("old.rs"), "v1\n").unwrap();
    git(root, &["add", "-A"]);
    git(root, &["commit", "-qm", "test: base"]);
    let base = rev(root, "HEAD");
    git(root, &["mv", "old.rs", "new.rs"]);
    git(root, &["commit", "-qm", "test: rename"]);

    let selection = discover_selection(root, false, Some(&base));
    assert_eq!(selection.paths(), vec!["new.rs", "old.rs"]);
}

/// A repository without commits cannot resolve a range and fails closed.
#[test]
fn discover_selection_fails_closed_without_commits() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    git(root, &["init", "-q"]);
    std::fs::write(root.join("main.rs"), "v1\n").unwrap();
    git(root, &["add", "-A"]);

    let selection = discover_selection(root, false, Some("HEAD"));
    assert!(selection.changed.discovery_failed);
    assert!(selection.range.is_none());
}
