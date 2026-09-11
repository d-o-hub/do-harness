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

/// Whether `root` lies inside a git working tree.
fn is_work_tree(root: &Path) -> bool {
    Command::new("git")
        .args(["rev-parse", "--is-inside-work-tree"])
        .current_dir(root)
        .output()
        .is_ok_and(|out| {
            out.status.success() && String::from_utf8_lossy(&out.stdout).trim() == "true"
        })
}

/// Tracked staged and unstaged changes versus `HEAD`, NUL-separated.
///
/// An unborn `HEAD` (no commits yet) yields no tracked changes, which is
/// safe: every file is untracked and covered by [`untracked_files`].
fn tracked_changes(root: &Path) -> Vec<ChangedFile> {
    let output = Command::new("git")
        .args(["diff", "--name-status", "-z", "HEAD"])
        .current_dir(root)
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
                // Rename/copy records carry `<from>\0<to>\0`.
                let from = records.next();
                let to = records.next();
                if let (Some(from), Some(to)) = (from, to) {
                    files.push(ChangedFile {
                        path: to,
                        kind: ChangeKind::Renamed { from },
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
    let output = Command::new("git")
        .args(["ls-files", "--others", "--exclude-standard", "-z"])
        .current_dir(root)
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
fn normalize(path: &str) -> String {
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

    /// Outside a git checkout discovery fails closed.
    #[test]
    fn discover_fails_closed_outside_git() {
        let dir = tempfile::tempdir().unwrap();
        let changed = discover(dir.path());
        assert!(changed.discovery_failed);
        assert!(changed.files.is_empty());
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

    /// A staged rename surfaces the new path with its origin.
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
            vec![ChangedFile {
                path: "new.rs".to_owned(),
                kind: ChangeKind::Renamed {
                    from: "old.rs".to_owned(),
                },
            }]
        );
    }

    /// Runs a git command inside `root`, asserting success.
    fn git(root: &Path, args: &[&str]) {
        let status = std::process::Command::new("git")
            .args(args)
            .current_dir(root)
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
