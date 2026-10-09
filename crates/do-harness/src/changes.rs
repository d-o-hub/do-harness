//! Working-tree change discovery for `--changed` selection.
//!
//! Changed files are the union of tracked modifications (staged and
//! unstaged, including deletions and renames) and untracked non-ignored
//! files. Discovery shells out to `git`; when repository state cannot be
//! determined (not a git checkout, `git` missing), discovery reports failure
//! and callers must fail closed by selecting every sensor.

use std::path::Path;
use std::process::Command;

/// How a path differs from `HEAD`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChangeKind {
    /// Staged new file.
    Added,
    /// Content differs from `HEAD` (staged, unstaged, or both).
    Modified,
    /// Tracked file removed from the working tree.
    Deleted,
    /// Tracked file moved; `from` is the previous path.
    Renamed {
        /// Previous repository-relative path.
        from: String,
    },
    /// Present on disk but unknown to git (never `HEAD`-listed).
    Untracked,
}

/// One changed repository-relative path (`/`-separated, sorted by caller).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangedFile {
    /// Repository-relative path with `/` separators.
    pub path: String,
    /// How the path differs from `HEAD`.
    pub kind: ChangeKind,
}

/// Discovered working-tree state for `--changed` selection.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ChangedFiles {
    /// Changed paths in sorted order.
    pub files: Vec<ChangedFile>,
    /// True when repository state could not be determined; callers must
    /// fail closed (select every sensor) instead of skipping.
    pub discovery_failed: bool,
}

impl ChangedFiles {
    /// Repository-relative paths, in sorted order.
    #[must_use]
    pub fn paths(&self) -> Vec<&str> {
        self.files.iter().map(|f| f.path.as_str()).collect()
    }
}

/// Discovers changed files under `root`.
///
/// Returns [`ChangedFiles::default`] with `discovery_failed` set when `root`
/// is not inside a git working tree or `git` cannot run. A repository
/// without commits yet is not a failure: every file is untracked, so the
/// untracked set alone drives selection.
#[must_use]
pub fn discover(root: &Path) -> ChangedFiles {
    if !is_work_tree(root) {
        return ChangedFiles {
            files: Vec::new(),
            discovery_failed: true,
        };
    }
    let mut files = tracked_changes(root);
    files.extend(untracked_files(root));
    files.sort_by(|a, b| a.path.cmp(&b.path));
    files.dedup_by(|a, b| a.path == b.path);
    ChangedFiles {
        files,
        discovery_failed: false,
    }
}

/// Environment variables through which `git` selects which repository and
/// history it reads.
///
/// Git exports these to hooks and documents the clearing recipe itself —
/// `githooks(5)`: "If your hook needs to invoke Git commands in a foreign
/// repository ... it should clear these environment variables", with
/// `unset $(git rev-parse --local-env-vars)` as the example. This list is
/// `git rev-parse --local-env-vars` plus `GIT_CEILING_DIRECTORIES`,
/// `GIT_NAMESPACE`, and `GIT_DISCOVERY_ACROSS_FILESYSTEM` (which steer
/// repository discovery and ref visibility), minus the `GIT_CONFIG*` trio:
/// those carry the caller's `-c` configuration (`safe.directory` among it)
/// rather than a repository identity, and dropping them would make the
/// harness ignore deliberate per-invocation configuration.
///
/// Left in place, a git call silently targets another repository, or reports a
/// rewritten history, instead of `root`; `git_view_env_covers_gits_local_environment`
/// keeps the coverage honest, and the generated scripts
/// (`templates/scripts/check-*.sh`) and the test fixtures
/// (`tests/support/mod.rs`) clear the same set.
pub(crate) const GIT_VIEW_ENV: [&str; 15] = [
    "GIT_DIR",
    "GIT_WORK_TREE",
    "GIT_IMPLICIT_WORK_TREE",
    "GIT_INDEX_FILE",
    "GIT_OBJECT_DIRECTORY",
    "GIT_ALTERNATE_OBJECT_DIRECTORIES",
    "GIT_CEILING_DIRECTORIES",
    "GIT_DISCOVERY_ACROSS_FILESYSTEM",
    "GIT_NAMESPACE",
    "GIT_PREFIX",
    "GIT_COMMON_DIR",
    "GIT_SHALLOW_FILE",
    "GIT_GRAFT_FILE",
    "GIT_REPLACE_REF_BASE",
    "GIT_NO_REPLACE_OBJECTS",
];

/// `GIT_CONFIG*` variables git also publishes as "local", deliberately kept:
/// they carry the caller's `-c` configuration instead of a repository
/// identity. Test-only: production code never needs to name them.
#[cfg(test)]
pub(crate) const GIT_VIEW_ENV_EXEMPT: [&str; 3] =
    ["GIT_CONFIG", "GIT_CONFIG_COUNT", "GIT_CONFIG_PARAMETERS"];

/// Removes [`GIT_VIEW_ENV`] from `command`.
pub(crate) fn clear_git_view(command: &mut Command) -> &mut Command {
    for key in GIT_VIEW_ENV {
        command.env_remove(key);
    }
    command
}

/// Builds a `git` command rooted at `root` with hook-inherited repository
/// environment removed.
///
/// Clearing [`GIT_VIEW_ENV`] keeps `--root` authoritative and makes change
/// discovery deterministic regardless of how the CLI was invoked.
pub(crate) fn git_command(root: &Path) -> Command {
    let mut command = Command::new("git");
    command.current_dir(root);
    clear_git_view(&mut command);
    command
}

/// Whether `root` lies inside a git working tree.
fn is_work_tree(root: &Path) -> bool {
    git_command(root)
        .args(["rev-parse", "--is-inside-work-tree"])
        .output()
        .is_ok_and(|out| {
            out.status.success() && String::from_utf8_lossy(&out.stdout).trim() == "true"
        })
}

/// Returns the current git branch name, when `root` lies inside a git working
/// tree and is on an attached branch. Returns `None` on detached HEAD or outside
/// git.
pub(crate) fn current_branch(root: &Path) -> Option<String> {
    let output = git_command(root)
        .args(["branch", "--show-current"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let name = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    (!name.is_empty()).then_some(name)
}

/// Tracked staged and unstaged changes versus `HEAD`, NUL-separated.
///
/// An unborn `HEAD` (no commits yet) yields no tracked changes, which is
/// safe: every file is untracked and covered by [`untracked_files`].
fn tracked_changes(root: &Path) -> Vec<ChangedFile> {
    let output = git_command(root)
        .args(["diff", "--name-status", "-z", "HEAD"])
        .output();
    let Ok(output) = output else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new();
    }
    // NUL-separated records: `<STATUS>\0<path>\0`, renames add the source
    // path (`R100\0<from>\0<to>\0`).
    let mut records = output
        .stdout
        .split(|b| *b == 0)
        .filter_map(|chunk| {
            let text = std::str::from_utf8(chunk).ok()?;
            (!text.is_empty()).then_some(normalize(text))
        })
        .peekable();
    let mut files = Vec::new();
    while let Some(status) = records.next() {
        let kind = status.chars().next().unwrap_or('?');
        match kind {
            'A' => {
                if let Some(path) = records.next() {
                    files.push(ChangedFile {
                        path,
                        kind: ChangeKind::Added,
                    });
                }
            }
            'M' | 'T' => {
                if let Some(path) = records.next() {
                    files.push(ChangedFile {
                        path,
                        kind: ChangeKind::Modified,
                    });
                }
            }
            'D' => {
                if let Some(path) = records.next() {
                    files.push(ChangedFile {
                        path,
                        kind: ChangeKind::Deleted,
                    });
                }
            }
            'R' | 'C' => {
                // Rename/copy records carry `<from>\0<to>\0`. Both ends are
                // recorded because change-aware selection matches on
                // `paths()`: a file moved out of a watched tree must trigger
                // that tree's sensors on the deletion side, exactly like a
                // plain delete does.
                let from = records.next();
                let to = records.next();
                if let (Some(from), Some(to)) = (from, to) {
                    files.push(ChangedFile {
                        path: to,
                        kind: ChangeKind::Renamed { from: from.clone() },
                    });
                    files.push(ChangedFile {
                        path: from,
                        kind: ChangeKind::Deleted,
                    });
                }
            }
            _ => {
                // Unknown status letters (e.g. `U` for unmerged): keep the
                // following path as modified rather than dropping it.
                if let Some(path) = records.next() {
                    files.push(ChangedFile {
                        path,
                        kind: ChangeKind::Modified,
                    });
                }
            }
        }
    }
    files
}

/// Untracked non-ignored files, NUL-separated.
fn untracked_files(root: &Path) -> Vec<ChangedFile> {
    let output = git_command(root)
        .args(["ls-files", "--others", "--exclude-standard", "-z"])
        .output();
    let Ok(output) = output else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new();
    }
    output
        .stdout
        .split(|b| *b == 0)
        .filter_map(|chunk| {
            let text = std::str::from_utf8(chunk).ok()?;
            (!text.is_empty()).then_some(ChangedFile {
                path: normalize(text),
                kind: ChangeKind::Untracked,
            })
        })
        .collect()
}

/// Normalizes a git-reported path: `/` separators on every OS, no `./`.
pub(crate) fn normalize(path: &str) -> String {
    let path = path.replace('\\', "/");
    path.strip_prefix("./").unwrap_or(&path).to_owned()
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    /// `normalize` unifies separators and strips leading `./`.
    #[test]
    fn normalize_unifies_separators() {
        assert_eq!(normalize("a/b"), "a/b");
        assert_eq!(normalize("a\\b"), "a/b");
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
}
