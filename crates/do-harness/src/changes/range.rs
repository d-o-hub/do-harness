//! `--since` range selection: the committed diff a branch introduces.
//!
//! `merge-base(<ref>, HEAD)..HEAD` is the change set a branch review cares
//! about, so a clean checkout of an already-committed branch still selects the
//! sensors its commits touch. Every failure to resolve a revision, the merge
//! base, or the diff fails closed through [`super::DiscoveryError`], never as
//! an empty change set.

use std::path::Path;

use super::{
    ChangedFile, ChangedFiles, DiscoveryError, discover, git_command, git_label, git_output,
    parse_name_status, require_work_tree,
};

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
    /// Changed paths and any discovery failure, sorted and deduplicated.
    pub changed: ChangedFiles,
    /// Resolved revisions for a `--since` selection.
    pub range: Option<ResolvedRange>,
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
            range: None,
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
                if selection.changed.discovery_error.is_none() {
                    selection.changed.discovery_error = working.discovery_error;
                }
            }
            selection
        }
    }
}

/// Resolves and diffs `merge-base(<base_ref>, HEAD)..HEAD`, failing closed.
fn range_selection(root: &Path, base_ref: &str, mode: SelectionMode) -> ChangeSelection {
    match range_inner(root, base_ref) {
        Ok((files, range)) => ChangeSelection {
            mode,
            changed: ChangedFiles {
                files,
                discovery_failed: false,
                discovery_error: None,
            },
            range: Some(range),
        },
        Err(error) => ChangeSelection {
            mode,
            changed: ChangedFiles {
                files: Vec::new(),
                discovery_failed: true,
                discovery_error: Some(error.to_string()),
            },
            range: None,
        },
    }
}

/// Resolves the range revisions and returns the paths they changed.
fn range_inner(
    root: &Path,
    base_ref: &str,
) -> Result<(Vec<ChangedFile>, ResolvedRange), DiscoveryError> {
    require_work_tree(root)?;
    let base_sha = resolve_commit(root, base_ref)?;
    let head_sha = resolve_commit(root, "HEAD")?;
    let merge_base_sha = merge_base(root, &base_sha, &head_sha)?;
    let files = diff_range(root, &merge_base_sha, &head_sha)?;
    Ok((
        files,
        ResolvedRange {
            requested: base_ref.to_owned(),
            base_sha,
            head_sha,
            merge_base_sha,
        },
    ))
}

/// Changed paths between two resolved commits.
///
/// A committed range uses the same record format as the working-tree diff, so
/// renames and deletions are reported identically on both sides.
fn diff_range(root: &Path, from: &str, to: &str) -> Result<Vec<ChangedFile>, DiscoveryError> {
    let args = ["diff", "--name-status", "-z", from, to];
    let output = git_output(root, &args)?;
    parse_name_status(&output.stdout, &git_label(&args))
}

/// Resolves `rev` to a full commit SHA.
fn resolve_commit(root: &Path, rev: &str) -> Result<String, DiscoveryError> {
    let spec = format!("{rev}^{{commit}}");
    let args = [
        "rev-parse",
        "--verify",
        "--quiet",
        "--end-of-options",
        spec.as_str(),
    ];
    let command = git_label(&args);
    let output = git_command(root)
        .args(args)
        .output()
        .map_err(|err| DiscoveryError::new(&command, err.to_string()))?;
    let sha = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if !output.status.success() || sha.is_empty() {
        return Err(DiscoveryError::new(
            command,
            format!("'{rev}' does not resolve to a commit"),
        ));
    }
    Ok(sha)
}

/// Merge base of two resolved commits, or a diagnostic for unrelated history.
fn merge_base(root: &Path, base: &str, head: &str) -> Result<String, DiscoveryError> {
    let args = ["merge-base", base, head];
    let command = git_label(&args);
    let output = git_output(root, &args)?;
    let sha = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if sha.is_empty() {
        return Err(DiscoveryError::new(command, "reported no merge base"));
    }
    Ok(sha)
}
