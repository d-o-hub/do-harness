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
    let error = changed.discovery_error.expect("diagnostic");
    assert!(error.contains("git rev-parse"), "{error}");
}

/// A repository with no commit yet reports staged and untracked files.
#[test]
fn discover_unborn_head_reports_staged_and_untracked() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    git(root, &["init", "-q"]);
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(root.join("src/lib.rs"), "pub fn f() {}\n").unwrap();
    git(root, &["add", "src/lib.rs"]);
    std::fs::write(root.join("notes.md"), "notes\n").unwrap();

    let changed = discover(root);
    assert!(!changed.discovery_failed, "{:?}", changed.discovery_error);
    assert_eq!(
        changed.files,
        vec![
            ChangedFile {
                path: "notes.md".to_owned(),
                kind: ChangeKind::Untracked,
            },
            ChangedFile {
                path: "src/lib.rs".to_owned(),
                kind: ChangeKind::Added,
            },
        ]
    );
}

/// Intent-to-add entries are index paths and must be discovered.
#[test]
fn discover_unborn_head_includes_intent_to_add() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    git(root, &["init", "-q"]);
    std::fs::write(root.join("intent.rs"), "fn main() {}\n").unwrap();
    git(root, &["add", "-N", "intent.rs"]);

    let changed = discover(root);
    assert!(!changed.discovery_failed, "{:?}", changed.discovery_error);
    assert!(
        changed.files.iter().any(|file| file.path == "intent.rs"),
        "intent-to-add path missing: {:?}",
        changed.files
    );
}

/// A clean committed tree is a successful empty change set, not a failure.
#[test]
fn discover_clean_tree_is_empty_and_successful() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    git(root, &["init", "-q"]);
    std::fs::write(root.join("main.rs"), "fn main() {}\n").unwrap();
    git(root, &["add", "-A"]);
    git(root, &["commit", "-qm", "test: base"]);

    let changed = discover(root);
    assert!(!changed.discovery_failed);
    assert_eq!(changed.files.len(), 0);
    assert_eq!(changed.discovery_error, None);
}

/// A failing git query fails closed with the git diagnostic, never an empty set.
#[test]
fn discover_fails_closed_when_git_diff_fails() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    git(root, &["init", "-q"]);
    std::fs::write(root.join("main.rs"), "fn main() {}\n").unwrap();
    git(root, &["add", "-A"]);
    git(root, &["commit", "-qm", "test: base"]);
    std::fs::write(root.join(".git/index"), "corrupt").unwrap();

    let changed = discover(root);
    assert!(changed.discovery_failed);
    assert_eq!(changed.files.len(), 0);
    let error = changed.discovery_error.expect("diagnostic");
    assert!(error.contains("git diff --name-status -z HEAD"), "{error}");
    assert!(error.contains("index"), "{error}");
}

/// A non-UTF-8 path cannot be matched against globs, so discovery fails closed.
#[cfg(unix)]
#[test]
fn discover_fails_closed_on_non_utf8_paths() {
    use std::os::unix::ffi::OsStrExt;

    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    git(root, &["init", "-q"]);
    std::fs::write(root.join("ok.rs"), "fn main() {}\n").unwrap();
    let name = std::ffi::OsStr::from_bytes(b"bad\xff.rs");
    std::fs::write(root.join(name), "x\n").unwrap();

    let changed = discover(root);
    assert!(changed.discovery_failed);
    let error = changed.discovery_error.expect("diagnostic");
    assert!(error.contains("UTF-8"), "{error}");
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
