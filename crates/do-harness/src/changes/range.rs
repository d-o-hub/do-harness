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

/// The `--since` spelling that resolves the base ref from the CI environment.
///
/// It is opt-in on purpose: `--since` must stay explicit so a green result can
/// never come from an accidental empty diff (#551). A CI job asks for the
/// provider's own target branch by name instead of hand-writing a vendor
/// variable into every snippet.
pub const CI_SINCE_SENTINEL: &str = "@ci";

/// Environment variables that name a pull/merge request's base branch, in
/// precedence order: `(provider, variable)`.
///
/// Only variables that name a *base branch* are listed. A vendor variable that
/// names the source branch is deliberately absent: selecting against the
/// branch under test would diff the branch against itself.
const CI_BASE_VARIABLES: [(&str, &str); 7] = [
    ("GitHub Actions", "GITHUB_BASE_REF"),
    ("GitLab CI", "CI_MERGE_REQUEST_TARGET_BRANCH_NAME"),
    ("Azure Pipelines", "SYSTEM_PULLREQUEST_TARGETBRANCH"),
    ("Bitbucket Pipelines", "BITBUCKET_PR_DESTINATION_BRANCH"),
    ("Buildkite", "BUILDKITE_PULL_REQUEST_BASE_BRANCH"),
    ("Jenkins", "CHANGE_TARGET"),
    ("Woodpecker CI", "CI_COMMIT_TARGET_BRANCH"),
];

/// Whether the requested `--since` spelling asks for CI base resolution.
#[must_use]
pub fn is_ci_since(requested: &str) -> bool {
    requested == CI_SINCE_SENTINEL
}

/// Resolves `@ci` to a locally resolvable remote-tracking ref.
///
/// Returns the branch name exactly as the provider advertised it after
/// resolution, so the caller can record both the requested spelling and the
/// ref that produced the range. Explicit spellings pass through unchanged.
fn resolve_since(root: &Path, requested: &str) -> Result<String, DiscoveryError> {
    if !is_ci_since(requested) {
        return Ok(requested.to_owned());
    }
    let Some((provider, variable, branch)) =
        CI_BASE_VARIABLES.iter().find_map(|(provider, name)| {
            let raw = std::env::var(name).ok()?;
            let branch = normalize_base_branch(&raw);
            (!branch.is_empty()).then_some((*provider, *name, branch))
        })
    else {
        return Err(DiscoveryError::new(
            CI_SINCE_SENTINEL,
            format!(
                "no known CI base-branch variable is set (looked for {}); pass --since <REF> explicitly",
                CI_BASE_VARIABLES
                    .iter()
                    .map(|(_, name)| *name)
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        ));
    };
    // The provider names a branch, not a ref: prefer the remote-tracking ref,
    // then the plain branch, then the fully qualified remote ref. A shallow
    // clone has none of them, and that must fail closed rather than fall back
    // to a full run.
    let candidates = [
        format!("origin/{branch}"),
        branch.clone(),
        format!("refs/remotes/origin/{branch}"),
    ];
    for candidate in candidates {
        if resolve_commit(root, &candidate).is_ok() {
            return Ok(candidate);
        }
    }
    Err(DiscoveryError::new(
        CI_SINCE_SENTINEL,
        format!(
            "{provider} reports base branch '{branch}' ({variable}) but no local ref resolves for it; \
             fetch it first (`git fetch --depth=0 origin {branch}`, or checkout with fetch-depth: 0)"
        ),
    ))
}

/// Strips a fully qualified ref prefix from a provider's branch name.
fn normalize_base_branch(raw: &str) -> String {
    let branch = raw.trim();
    branch
        .strip_prefix("refs/heads/")
        .or_else(|| branch.strip_prefix("refs/"))
        .unwrap_or(branch)
        .to_owned()
}

/// Resolves and diffs `merge-base(<base_ref>, HEAD)..HEAD`, failing closed.
fn range_selection(root: &Path, base_ref: &str, mode: SelectionMode) -> ChangeSelection {
    match resolve_since(root, base_ref).and_then(|resolved| range_inner(root, base_ref, &resolved))
    {
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
///
/// `requested` is the spelling the user typed (`@ci` included) and is what the
/// report records; `base_ref` is the ref it resolved to.
fn range_inner(
    root: &Path,
    requested: &str,
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
            requested: requested.to_owned(),
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
