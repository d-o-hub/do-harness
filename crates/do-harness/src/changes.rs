//! Working-tree change discovery for `--changed` selection.
//!
//! Changed files are the union of tracked modifications (staged and
//! unstaged, including deletions and renames) and untracked non-ignored
//! files. Discovery shells out to `git`; when repository state cannot be
//! determined (not a git checkout, `git` missing, a failing git command, a
//! non-UTF-8 path), discovery reports the failure and its diagnostic, and
//! callers must fail closed by selecting every sensor.

use std::path::Path;
use std::process::{Command, Output};

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
    /// Why discovery failed, when it did: the failing git command and its
    /// diagnostic. Selection already fails closed; this is for reports.
    pub discovery_error: Option<String>,
}

impl ChangedFiles {
    /// Repository-relative paths, in sorted order.
    #[must_use]
    pub fn paths(&self) -> Vec<&str> {
        self.files.iter().map(|f| f.path.as_str()).collect()
    }
}

/// A failed change-discovery probe: the git invocation and its diagnostic.
///
/// Carries the cause so fail-closed selection can name it instead of
/// reporting an unexplained empty change set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DiscoveryError {
    command: String,
    detail: String,
}

impl DiscoveryError {
    fn new(command: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            command: command.into(),
            detail: detail.into(),
        }
    }

    /// Maps a completed but unsuccessful git invocation to an error.
    fn from_output(command: &str, output: &Output) -> Self {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stderr = stderr.trim();
        if stderr.is_empty() {
            Self::new(command, output.status.to_string())
        } else {
            Self::new(command, stderr)
        }
    }
}

impl std::fmt::Display for DiscoveryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} failed: {}", self.command, self.detail)
    }
}

/// Discovers changed files under `root`.
///
/// Returns an empty change set with `discovery_failed` and `discovery_error`
/// set when the working tree cannot be inspected: `root` is not inside a git
/// working tree, `git` cannot run, or any git query fails. A repository
/// without commits is not a failure: staged index paths are additions and
/// untracked files are discovered as usual.
#[must_use]
pub fn discover(root: &Path) -> ChangedFiles {
    match discover_inner(root) {
        Ok(files) => ChangedFiles {
            files,
            discovery_failed: false,
            discovery_error: None,
        },
        Err(error) => ChangedFiles {
            files: Vec::new(),
            discovery_failed: true,
            discovery_error: Some(error.to_string()),
        },
    }
}

/// Change discovery that reports the first failing step instead of swallowing it.
fn discover_inner(root: &Path) -> Result<Vec<ChangedFile>, DiscoveryError> {
    require_work_tree(root)?;
    let mut files = match head_state(root)? {
        HeadState::Born => tracked_changes(root)?,
        HeadState::Unborn => index_paths(root)?,
    };
    files.extend(untracked_files(root)?);
    files.sort_by(|a, b| a.path.cmp(&b.path));
    files.dedup_by(|a, b| a.path == b.path);
    Ok(files)
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

/// Runs `git <args>` in `root`, failing on spawn errors and non-zero exits.
fn git_output(root: &Path, args: &[&str]) -> Result<Output, DiscoveryError> {
    let command = git_label(args);
    let output = git_command(root)
        .args(args)
        .output()
        .map_err(|err| DiscoveryError::new(&command, err.to_string()))?;
    if output.status.success() {
        Ok(output)
    } else {
        Err(DiscoveryError::from_output(&command, &output))
    }
}

/// Human-readable form of a git invocation, used in diagnostics.
fn git_label(args: &[&str]) -> String {
    format!("git {}", args.join(" "))
}

/// Requires `root` to lie inside a git working tree.
fn require_work_tree(root: &Path) -> Result<(), DiscoveryError> {
    let args = ["rev-parse", "--is-inside-work-tree"];
    let output = git_output(root, &args)?;
    let inside = String::from_utf8_lossy(&output.stdout);
    if inside.trim() == "true" {
        Ok(())
    } else {
        Err(DiscoveryError::new(
            git_label(&args),
            format!("reported `{}` instead of a work tree", inside.trim()),
        ))
    }
}

/// Whether `HEAD` holds a commit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HeadState {
    /// `HEAD` resolves to a commit; tracked changes come from `git diff`.
    Born,
    /// No commit exists yet; the index holds the only tracked state.
    Unborn,
}

/// Classifies `HEAD` as born or unborn.
///
/// An unborn `HEAD` is a symbolic ref to a branch with no commit, so
/// `rev-parse --verify HEAD` cannot resolve it while `symbolic-ref` succeeds.
/// Any other unresolvable `HEAD` is a repository error and fails closed.
fn head_state(root: &Path) -> Result<HeadState, DiscoveryError> {
    let command = "git rev-parse --verify HEAD";
    let head = git_command(root)
        .args(["rev-parse", "--verify", "HEAD"])
        .output()
        .map_err(|err| DiscoveryError::new(command, err.to_string()))?;
    if head.status.success() {
        return Ok(HeadState::Born);
    }
    let symbolic = git_command(root)
        .args(["symbolic-ref", "--quiet", "HEAD"])
        .output()
        .map_err(|err| DiscoveryError::new("git symbolic-ref --quiet HEAD", err.to_string()))?;
    if symbolic.status.success() {
        Ok(HeadState::Unborn)
    } else {
        Err(DiscoveryError::from_output(command, &head))
    }
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

/// Index paths in a repository with an unborn `HEAD`.
///
/// Every staged path is an addition relative to the empty history.
/// `git ls-files --cached` includes intent-to-add entries; deletions cannot
/// exist before the first commit.
fn index_paths(root: &Path) -> Result<Vec<ChangedFile>, DiscoveryError> {
    let args = ["ls-files", "--cached", "-z"];
    let output = git_output(root, &args)?;
    parse_paths(&output.stdout, &ChangeKind::Added, &git_label(&args))
}

/// Tracked staged and unstaged changes versus `HEAD`, NUL-separated.
fn tracked_changes(root: &Path) -> Result<Vec<ChangedFile>, DiscoveryError> {
    let args = ["diff", "--name-status", "-z", "HEAD"];
    let command = git_label(&args);
    let output = git_output(root, &args)?;
    // NUL-separated records: `<STATUS>\0<path>\0`, renames add the source
    // path (`R100\0<from>\0<to>\0`).
    let records: Vec<&str> = output
        .stdout
        .split(|byte| *byte == 0)
        .filter(|chunk| !chunk.is_empty())
        .map(|chunk| {
            std::str::from_utf8(chunk).map_err(|_| {
                DiscoveryError::new(&command, "reported a path that is not valid UTF-8")
            })
        })
        .collect::<Result<_, _>>()?;
    let mut records = records.into_iter();
    let mut files = Vec::new();
    while let Some(status) = records.next() {
        let Some(letter) = status.chars().next() else {
            return Err(DiscoveryError::new(
                &command,
                "reported an empty status record",
            ));
        };
        let mut next_path = |what: &str| -> Result<String, DiscoveryError> {
            records.next().map(normalize).ok_or_else(|| {
                DiscoveryError::new(&command, format!("missing {what} path after `{status}`"))
            })
        };
        match letter {
            'A' => files.push(ChangedFile {
                path: next_path("added")?,
                kind: ChangeKind::Added,
            }),
            'M' | 'T' => files.push(ChangedFile {
                path: next_path("modified")?,
                kind: ChangeKind::Modified,
            }),
            'D' => files.push(ChangedFile {
                path: next_path("deleted")?,
                kind: ChangeKind::Deleted,
            }),
            'R' | 'C' => {
                // Rename/copy records carry `<from>\0<to>\0`. Both ends are
                // recorded because change-aware selection matches on
                // `paths()`: a file moved out of a watched tree must trigger
                // that tree's sensors on the deletion side, exactly like a
                // plain delete does.
                let from = next_path("source")?;
                let to = next_path("destination")?;
                files.push(ChangedFile {
                    path: to,
                    kind: ChangeKind::Renamed { from: from.clone() },
                });
                files.push(ChangedFile {
                    path: from,
                    kind: ChangeKind::Deleted,
                });
            }
            _ => {
                // Unknown status letters (e.g. `U` for unmerged): keep the
                // following path as modified rather than dropping it.
                files.push(ChangedFile {
                    path: next_path("changed")?,
                    kind: ChangeKind::Modified,
                });
            }
        }
    }
    Ok(files)
}

/// Untracked non-ignored files, NUL-separated.
fn untracked_files(root: &Path) -> Result<Vec<ChangedFile>, DiscoveryError> {
    let args = ["ls-files", "--others", "--exclude-standard", "-z"];
    let output = git_output(root, &args)?;
    parse_paths(&output.stdout, &ChangeKind::Untracked, &git_label(&args))
}

/// Parses NUL-separated git path output, rejecting non-UTF-8 paths.
fn parse_paths(
    stdout: &[u8],
    kind: &ChangeKind,
    command: &str,
) -> Result<Vec<ChangedFile>, DiscoveryError> {
    stdout
        .split(|byte| *byte == 0)
        .filter(|chunk| !chunk.is_empty())
        .map(|chunk| {
            let text = std::str::from_utf8(chunk).map_err(|_| {
                DiscoveryError::new(command, "reported a path that is not valid UTF-8")
            })?;
            Ok(ChangedFile {
                path: normalize(text),
                kind: kind.clone(),
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
