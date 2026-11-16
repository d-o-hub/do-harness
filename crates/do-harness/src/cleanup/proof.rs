//! Merge proofs: why a branch's work is already contained in the base branch.
//!
//! The proof must match this repository's merge strategy. `gh pr merge
//! --squash` rewrites history, so `git merge-base --is-ancestor` reports every
//! merged branch as unmerged. The authoritative proof is therefore the merged
//! pull request, bound to the worktree's current head; ancestry and tree
//! equality are local fallbacks that may only promote a branch with no open
//! pull request.

use std::path::Path;

use anyhow::Result;
use serde::{Deserialize, Serialize};

use super::Reason;
use super::discover::{Base, Entry};
use crate::changes::git_command;
use crate::gh;

/// How the merge was proven.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProofKind {
    /// A merged pull request whose `headRefOid` equals the worktree's head.
    PrMerged,
    /// The head is an ancestor of the base (fast-forward or merge commit).
    Ancestor,
    /// The head's tree is identical to the base's tree.
    TreeAbsorbed,
    /// No proof was available.
    Unproven,
}

/// The recorded proof for one candidate.
#[derive(Debug, Clone, Serialize)]
pub struct MergeProof {
    /// Proof kind.
    pub kind: ProofKind,
    /// Pull request number when the proof came from one.
    pub pr: Option<u64>,
    /// Merge timestamp when the proof came from a merged pull request.
    pub merged_at: Option<String>,
}

impl MergeProof {
    /// The no-proof value.
    #[must_use]
    pub fn unproven() -> Self {
        Self {
            kind: ProofKind::Unproven,
            pr: None,
            merged_at: None,
        }
    }
}

/// Outcome of proving one candidate.
#[derive(Debug, Clone)]
pub enum Proof {
    /// The branch is provably merged.
    Merged(MergeProof),
    /// The proof is missing, stale, or contradicted.
    Refused(Reason),
}

/// One row of `gh pr list --json number,headRefOid,mergedAt`.
#[derive(Debug, Deserialize)]
struct Pull {
    number: u64,
    #[serde(rename = "headRefOid")]
    head_ref_oid: String,
    #[serde(rename = "mergedAt")]
    merged_at: Option<String>,
}

/// Proves whether `entry` is merged into `base`.
///
/// `explicit` records that the operator named this worktree, which is the only
/// case where a detached head may be promoted by tree equality. `optimistic`
/// permits a local proof when `gh` cannot answer, and is reported by the caller
/// as the documented risk it is.
pub fn prove(root: &Path, entry: &Entry, explicit: bool, optimistic: bool) -> Proof {
    if entry.detached {
        return if explicit && tree_absorbed(root, &entry.head) {
            Proof::Merged(MergeProof {
                kind: ProofKind::TreeAbsorbed,
                pr: None,
                merged_at: None,
            })
        } else {
            Proof::Refused(Reason::Detached)
        };
    }
    let Some(branch) = entry.branch.as_deref() else {
        return Proof::Refused(Reason::Detached);
    };
    let open = match pulls(root, branch, "open") {
        Ok(pulls) => pulls,
        Err(_) if optimistic => return local(root, entry),
        Err(_) => return Proof::Refused(Reason::Unproven),
    };
    if !open.is_empty() {
        return Proof::Refused(Reason::OpenPr);
    }
    let merged = match pulls(root, branch, "merged") {
        Ok(pulls) => pulls,
        Err(_) if optimistic => return local(root, entry),
        Err(_) => return Proof::Refused(Reason::Unproven),
    };
    if let Some(pull) = merged.iter().find(|pull| pull.head_ref_oid == entry.head) {
        return Proof::Merged(MergeProof {
            kind: ProofKind::PrMerged,
            pr: Some(pull.number),
            merged_at: pull.merged_at.clone(),
        });
    }
    if merged.is_empty() {
        local(root, entry)
    } else {
        // A merged pull request exists for the branch name, but it merged a
        // different head: the name was reused after the merge.
        Proof::Refused(Reason::ProofStale)
    }
}

/// Local fallbacks, only reached when no open pull request exists.
fn local(root: &Path, entry: &Entry) -> Proof {
    if ancestor(root, &entry.head) {
        return Proof::Merged(MergeProof {
            kind: ProofKind::Ancestor,
            pr: None,
            merged_at: None,
        });
    }
    if tree_absorbed(root, &entry.head) {
        return Proof::Merged(MergeProof {
            kind: ProofKind::TreeAbsorbed,
            pr: None,
            merged_at: None,
        });
    }
    Proof::Refused(Reason::Unmerged)
}

/// Lists pull requests for `branch` in the given state.
fn pulls(root: &Path, branch: &str, state: &str) -> Result<Vec<Pull>> {
    let limit = "10";
    gh::json::<Vec<Pull>>(
        root,
        &[
            "pr",
            "list",
            "--head",
            branch,
            "--state",
            state,
            "--limit",
            limit,
            "--json",
            "number,headRefOid,mergedAt",
        ],
        "listing pull requests for a cleanup candidate",
    )
}

/// Whether `head` is an ancestor of `origin/main`.
fn ancestor(root: &Path, head: &str) -> bool {
    exit_ok(root, &["merge-base", "--is-ancestor", head, "origin/main"])
}

/// Whether `head`'s tree equals `origin/main`'s tree.
fn tree_absorbed(root: &Path, head: &str) -> bool {
    exit_ok(root, &["diff", "--quiet", "origin/main", head])
}

/// Runs `git <args>` and reports whether it exited zero.
///
/// A spawn failure is `false`: no evidence is not evidence.
fn exit_ok(root: &Path, args: &[&str]) -> bool {
    git_command(root)
        .args(args)
        .output()
        .is_ok_and(|output| output.status.success())
}

/// The age after which a fetched `origin/main` is no longer trusted.
pub const MAX_BASE_AGE_SECONDS: i64 = 7 * 24 * 60 * 60;

/// Whether the base is fresh enough to prove against.
#[must_use]
pub fn fresh(base: &Base, now_unix: i64) -> bool {
    base.fetched_at_unix
        .is_some_and(|fetched| now_unix.saturating_sub(fetched) <= MAX_BASE_AGE_SECONDS)
}

/// Reads the worktree registry written by worktree creation.
///
/// # Errors
///
/// Returns an error when the file exists but cannot be parsed, which the caller
/// reports as `registry_unreadable` rather than as an empty registry.
pub fn registry(root: &Path) -> Result<Vec<Record>> {
    let path = root.join(".do-harness/worktrees.json");
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error.into()),
    };
    let file: RegistryFile = serde_json::from_str(&text)?;
    Ok(file.worktrees)
}

/// One recorded worktree-to-target mapping.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Record {
    /// Worktree the record describes.
    pub worktree: std::path::PathBuf,
    /// Redirected build tree for that worktree.
    pub target_dir: std::path::PathBuf,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RegistryFile {
    worktrees: Vec<Record>,
}

/// Resolves the target directory for `entry`.
///
/// An explicit `--target-dir` wins, then a registry record, then
/// `<worktree>/target`. A record or explicit path outside the worktree is only
/// honoured with `--include-shared`.
#[must_use]
pub fn target(
    entry: &Entry,
    record: Option<&Record>,
    explicit: &[std::path::PathBuf],
    include_shared: bool,
) -> TargetOutcome {
    let candidate = explicit
        .iter()
        .find(|path| super::guard::is_real_dir(path))
        .cloned()
        .or_else(|| record.map(|record| record.target_dir.clone()))
        .unwrap_or_else(|| entry.path.join("target"));
    let inside = candidate.starts_with(&entry.path);
    if !inside && !include_shared {
        return TargetOutcome::Redirected;
    }
    if !super::guard::is_real_dir(&candidate) {
        return TargetOutcome::Missing;
    }
    TargetOutcome::Target(candidate)
}

/// Result of resolving a candidate's build tree.
#[derive(Debug, Clone)]
pub enum TargetOutcome {
    /// The build tree to consider.
    Target(std::path::PathBuf),
    /// No build tree exists at the resolved location.
    Missing,
    /// The build tree is redirected outside the worktree without `--include-shared`.
    Redirected,
}
