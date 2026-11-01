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
