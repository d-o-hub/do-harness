//! Change discovery for `--changed` and `--since` selection.
//!
//! Changed files are the union of tracked modifications (staged and
//! unstaged, including deletions and renames) and untracked non-ignored
//! files. Discovery shells out to `git`; when repository state cannot be
//! determined (not a git checkout, `git` missing), discovery reports failure
//! and callers must fail closed by selecting every sensor.
//!
//! `--since <ref>` selects from the committed range
//! `merge-base(<ref>, HEAD)..HEAD` instead, so a clean checkout of an
//! already-committed branch still selects the sensors its commits touch.

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

/// Which inputs drove change-aware selection.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum SelectionMode {
    /// No change filtering: every candidate sensor is applicable.
    #[default]
    All,
    /// Working-tree changes versus `HEAD` (`--changed`).
    WorkingTree,
    /// Committed range from the merge base with `--since <ref>` to `HEAD`.
    Range,
    /// Union of working-tree and committed-range changes.
    WorkingTreeAndRange,
}

impl SelectionMode {
    /// Stable machine-readable spelling used in reports and evidence.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::WorkingTree => "working-tree",
            Self::Range => "range",
            Self::WorkingTreeAndRange => "working-tree+range",
        }
    }
}

/// Revisions a `--since` selection resolved to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedRange {
    /// Ref exactly as requested on the command line.
    pub requested: String,
    /// Commit the requested ref resolved to.
    pub base_sha: String,
    /// `HEAD` commit the range was compared against.
    pub head_sha: String,
    /// Merge base of `base_sha` and `head_sha`.
    pub merge_base_sha: String,
}

/// Change input for one selection, with the revisions that produced it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ChangeSelection {
    /// Which inputs drove selection.
    pub mode: SelectionMode,
    /// Changed paths, sorted and deduplicated.
    pub changed: ChangedFiles,
    /// Resolved revisions for a `--since` selection.
    pub range: Option<ResolvedRange>,
    /// Why discovery failed closed, when it did.
    pub diagnostic: Option<String>,
}

impl ChangeSelection {
    /// Repository-relative changed paths, in sorted order.
    #[must_use]
    pub fn paths(&self) -> Vec<&str> {
        self.changed.paths()
    }
}

/// Discovers the change input for `--changed` and/or `--since <ref>`.
///
/// `--since` compares `merge-base(<ref>, HEAD)` to `HEAD`, so a clean
/// checkout of a committed branch still selects the sensors its commits
/// touch. Combining both flags unions the two path sets. Any failure to
/// resolve the ref, `HEAD`, the merge base, or the diff fails closed with a
/// diagnostic instead of reporting an empty change set.
#[must_use]
pub fn discover_selection(root: &Path, changed: bool, since: Option<&str>) -> ChangeSelection {
    match (changed, since) {
        (false, None) => ChangeSelection::default(),
        (true, None) => ChangeSelection {
            mode: SelectionMode::WorkingTree,
            changed: discover(root),
            ..ChangeSelection::default()
        },
        (false, Some(base)) => range_selection(root, base, SelectionMode::Range),
        (true, Some(base)) => {
            let mut selection = range_selection(root, base, SelectionMode::WorkingTreeAndRange);
            let working = discover(root);
            selection.changed.files.extend(working.files);
            selection.changed.files.sort_by(|a, b| a.path.cmp(&b.path));
            selection.changed.files.dedup_by(|a, b| a.path == b.path);
            if working.discovery_failed {
                selection.changed.discovery_failed = true;
                selection
                    .diagnostic
                    .get_or_insert_with(|| "working-tree change discovery failed".to_owned());
            }
            selection
        }
    }
}

/// Resolves and diffs `merge-base(<base_ref>, HEAD)..HEAD`.
fn range_selection(root: &Path, base_ref: &str, mode: SelectionMode) -> ChangeSelection {
    let mut selection = ChangeSelection {
        mode,
        ..ChangeSelection::default()
    };
    if !is_work_tree(root) {
        return fail_closed(selection, "not a git working tree".to_owned());
    }
    let Some(base_sha) = resolve_commit(root, base_ref) else {
        return fail_closed(
            selection,
            format!("cannot resolve --since ref '{base_ref}' to a commit"),
        );
    };
    let Some(head_sha) = resolve_commit(root, "HEAD") else {
        return fail_closed(selection, "cannot resolve HEAD to a commit".to_owned());
    };
    let Some(merge_base_sha) = merge_base(root, &base_sha, &head_sha) else {
        return fail_closed(
            selection,
            format!("no merge base between '{base_ref}' and HEAD"),
        );
    };
    let Some(files) = diff_range(root, &merge_base_sha, &head_sha) else {
        return fail_closed(
            selection,
            format!("cannot diff {merge_base_sha}..{head_sha}"),
        );
    };
    selection.changed.files = files;
    selection.range = Some(ResolvedRange {
        requested: base_ref.to_owned(),
        base_sha,
        head_sha,
        merge_base_sha,
    });
    selection
}

/// Marks a selection as failed closed with a diagnostic.
fn fail_closed(mut selection: ChangeSelection, diagnostic: String) -> ChangeSelection {
    selection.changed.discovery_failed = true;
    selection.diagnostic = Some(diagnostic);
    selection
}

/// Resolves `rev` to a full commit SHA, or `None` when it does not exist.
fn resolve_commit(root: &Path, rev: &str) -> Option<String> {
    let output = git_command(root)
        .args(["rev-parse", "--verify", "--quiet", "--end-of-options"])
        .arg(format!("{rev}^{{commit}}"))
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let sha = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    (!sha.is_empty()).then_some(sha)
}

/// Merge base of two resolved commits, or `None` for unrelated histories.
fn merge_base(root: &Path, base: &str, head: &str) -> Option<String> {
    let output = git_command(root)
        .args(["merge-base", base, head])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let sha = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    (!sha.is_empty()).then_some(sha)
}

/// Changed paths between two resolved commits, or `None` on git failure.
fn diff_range(root: &Path, from: &str, to: &str) -> Option<Vec<ChangedFile>> {
    let output = git_command(root)
        .args(["diff", "--name-status", "-z", from, to])
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| parse_name_status(&output.stdout))
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
    parse_name_status(&output.stdout)
}

/// Parses NUL-separated `git diff --name-status -z` records.
fn parse_name_status(bytes: &[u8]) -> Vec<ChangedFile> {
    // NUL-separated records: `<STATUS>\0<path>\0`, renames add the source
    // path (`R100\0<from>\0<to>\0`).
    let mut records = bytes
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
    let path = if cfg!(windows) {
        path.replace('\\', "/")
    } else {
        path.to_owned()
    };
    path.strip_prefix("./").unwrap_or(&path).to_owned()
}

#[cfg(test)]
#[path = "changes_tests.rs"]
mod tests;
