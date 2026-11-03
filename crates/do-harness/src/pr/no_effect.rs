//! Merge-base tree comparison: does a revision introduce any effective change?
//!
//! [`effective_change`] answers a historical question — what the branch changed
//! since it forked — and is kept for review input and audit. [`merge_impact`]
//! answers the prospective question that decides redundant-PR detection:
//! would merging the head into the *current* base change the base tree?

use std::path::Path;

use anyhow::{Context, Result, bail};

use crate::changes::git_command;

/// Hex length of a SHA-1 object id.
const SHA1_HEX_LEN: usize = 40;
/// Hex length of a SHA-256 object id.
const SHA256_HEX_LEN: usize = 64;

/// Historical effective-change verdict for a base/head pair.
///
/// This is the *branch-change* verdict: the tree delta between the merge base
/// and the head. It does not prove that merging the head into the current base
/// would change the base tree; use [`merge_impact`] for that.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Effect {
    /// Head contains no tree delta beyond the merge base.
    NoEffect,
    /// Head differs from the merge base.
    HasEffect,
}

impl Effect {
    /// Whether an effective change remains.
    #[must_use]
    pub fn has_effect(self) -> bool {
        matches!(self, Self::HasEffect)
    }
}

/// Prospective merge-impact verdict for a base/head pair.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum MergeImpact {
    /// A clean merge result tree equals the current base tree.
    NoTreeEffect,
    /// A clean merge result tree differs from the current base tree.
    TreeEffect,
    /// No clean merge result could be modelled; never recommend closure.
    Unknown,
}

impl MergeImpact {
    /// Whether the merge is proven to leave the base tree unchanged.
    #[must_use]
    pub fn is_no_tree_effect(self) -> bool {
        matches!(self, Self::NoTreeEffect)
    }
}

impl std::fmt::Display for MergeImpact {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::NoTreeEffect => "NO_TREE_EFFECT",
            Self::TreeEffect => "TREE_EFFECT",
            Self::Unknown => "UNKNOWN",
        })
    }
}

/// Evidence behind one merge-impact verdict.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct MergeImpactReport {
    /// Merge-impact verdict.
    pub verdict: MergeImpact,
    /// How the verdict was obtained: `merge-tree` or `unavailable`.
    pub method: String,
    /// Base revision as requested.
    pub base: String,
    /// Head revision as requested.
    pub head: String,
    /// Resolved base commit, when available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_sha: Option<String>,
    /// Resolved head commit, when available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub head_sha: Option<String>,
    /// Tree written by the prospective merge, when the merge was modelled.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result_tree: Option<String>,
    /// Whether the base is the current remote base, not a stale local ref.
    pub base_verified: bool,
    /// Machine-readable evidence marker.
    pub evidence: String,
    /// Why the verdict is `UNKNOWN`, when it is.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// Non-fatal diagnostics, such as an unverified base.
    pub warnings: Vec<String>,
}

impl MergeImpactReport {
    /// Builds an `UNKNOWN` report for a merge that could not be modelled.
    #[must_use]
    pub fn unknown(base: &str, head: &str, reason: impl Into<String>) -> Self {
        Self {
            verdict: MergeImpact::Unknown,
            method: "unavailable".to_owned(),
            base: base.to_owned(),
            head: head.to_owned(),
            base_sha: None,
            head_sha: None,
            result_tree: None,
            base_verified: false,
            evidence: "merge-unmodelled".to_owned(),
            reason: Some(reason.into()),
            warnings: Vec::new(),
        }
    }
}

/// Candidate refs for a pull request's base branch, most authoritative first.
///
/// A PR's base is the *remote* branch, so the remote-tracking ref is the honest
/// local proxy. A same-named local branch is refreshed only by checking it out
/// or pulling it, and a stale one silently reports already-merged upstream
/// commits as this PR's change — it would also flip a no-effect verdict.
#[must_use]
pub fn base_refs(base: &str) -> [String; 2] {
    [format!("origin/{base}"), base.to_owned()]
}

/// Returns the merge base of `base` and `head`.
///
/// # Errors
///
/// Returns an error when either revision is missing or git cannot compute
/// the merge base.
pub fn merge_base(root: &Path, base: &str, head: &str) -> Result<String> {
    let output = git_command(root)
        .args(["merge-base", base, head])
        .output()
        .context("failed to run git merge-base")?;
    if !output.status.success() {
        bail!(
            "git merge-base {base} {head} failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    let sha = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if sha.is_empty() {
        bail!("git merge-base {base} {head} returned no commit");
    }
    Ok(sha)
}

/// Whether `head` introduces any tree delta beyond the merge base.
///
/// # Errors
///
/// Returns an error when either revision is missing or git cannot compare the
/// trees; callers must treat errors as "cannot determine", never as "no effect".
pub fn effective_change(root: &Path, base: &str, head: &str) -> Result<Effect> {
    let merge_commit = merge_base(root, base, head)?;
    let output = git_command(root)
        .args(["diff", "--quiet", &merge_commit, head, "--"])
        .output()
        .context("failed to run git diff")?;
    match output.status.code() {
        Some(0) => Ok(Effect::NoEffect),
        Some(1) => Ok(Effect::HasEffect),
        _ => bail!(
            "git diff --quiet {merge_commit} {head} failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ),
    }
}

/// Resolves `rev` to a full commit SHA.
///
/// # Errors
///
/// Returns an error when `rev` does not resolve to a commit.
pub fn commit_sha(root: &Path, rev: &str) -> Result<String> {
    let output = git_command(root)
        .args(["rev-parse", "--verify", &format!("{rev}^{{commit}}")])
        .output()
        .context("failed to run git rev-parse")?;
    if !output.status.success() {
        bail!(
            "git rev-parse {rev} failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    let sha = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if sha.is_empty() {
        bail!("git rev-parse {rev} returned no commit");
    }
    Ok(sha)
}

/// Whether `rev` resolves to a commit object in the repository.
#[must_use]
pub fn rev_exists(root: &Path, rev: &str) -> bool {
    git_command(root)
        .args(["cat-file", "-e", &format!("{rev}^{{commit}}")])
        .output()
        .is_ok_and(|out| out.status.success())
}

/// Resolves the tree object of `rev`.
///
/// # Errors
///
/// Returns an error when `rev` does not resolve to a commit tree.
fn tree_sha(root: &Path, rev: &str) -> Result<String> {
    let output = git_command(root)
        .args(["rev-parse", "--verify", &format!("{rev}^{{tree}}")])
        .output()
        .context("failed to run git rev-parse")?;
    if !output.status.success() {
        bail!(
            "git rev-parse {rev}^{{tree}} failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    let sha = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if sha.is_empty() {
        bail!("git rev-parse {rev}^{{tree}} returned no tree");
    }
    Ok(sha)
}

/// Whether `value` looks like a full git object id.
fn is_object_id(value: &str) -> bool {
    matches!(value.len(), SHA1_HEX_LEN | SHA256_HEX_LEN)
        && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

/// Computes the prospective merge impact of merging `head` into `base`.
///
/// Uses `git merge-tree --write-tree`, which writes only into the object
/// database: the worktree, index, branches, and checkout are untouched. A
/// conflict, an unresolvable revision, or a Git that predates `--write-tree`
/// yields [`MergeImpact::Unknown`] with a reason; it never yields
/// `NO_TREE_EFFECT`.
#[must_use]
pub fn merge_impact(root: &Path, base: &str, head: &str) -> MergeImpactReport {
    let mut report = MergeImpactReport {
        verdict: MergeImpact::Unknown,
        method: "unavailable".to_owned(),
        base: base.to_owned(),
        head: head.to_owned(),
        base_sha: None,
        head_sha: None,
        result_tree: None,
        base_verified: true,
        evidence: String::new(),
        reason: None,
        warnings: Vec::new(),
    };

    let base_sha = match commit_sha(root, base) {
        Ok(sha) => sha,
        Err(err) => {
            "base-unresolved".clone_into(&mut report.evidence);
            report.reason = Some(format!(
                "base revision `{base}` is unavailable: {err}; fetch it first (`git fetch origin {base}`)"
            ));
            return report;
        }
    };
    let head_sha = match commit_sha(root, head) {
        Ok(sha) => sha,
        Err(err) => {
            report.base_sha = Some(base_sha);
            "head-unresolved".clone_into(&mut report.evidence);
            report.reason = Some(format!(
                "head revision `{head}` is unavailable: {err}; fetch it first (`git fetch origin`)"
            ));
            return report;
        }
    };
    let base_tree = match tree_sha(root, &base_sha) {
        Ok(tree) => tree,
        Err(err) => {
            report.base_sha = Some(base_sha);
            report.head_sha = Some(head_sha);
            "base-tree-unresolved".clone_into(&mut report.evidence);
            report.reason = Some(format!("cannot resolve the base tree: {err}"));
            return report;
        }
    };
    report.base_sha = Some(base_sha.clone());
    report.head_sha = Some(head_sha.clone());

    let output = match git_command(root)
        .args(["merge-tree", "--write-tree", &base_sha, &head_sha])
        .output()
    {
        Ok(output) => output,
        Err(err) => {
            "merge-tree-unavailable".clone_into(&mut report.evidence);
            report.reason = Some(format!("git merge-tree could not be executed: {err}"));
            return report;
        }
    };
    let stdout = String::from_utf8_lossy(&output.stdout);
    let result_tree = stdout.lines().next().unwrap_or_default().trim().to_owned();
    let stderr = String::from_utf8_lossy(&output.stderr);
    match output.status.code() {
        Some(0) if is_object_id(&result_tree) => {
            "merge-tree".clone_into(&mut report.method);
            report.result_tree = Some(result_tree.clone());
            if result_tree == base_tree {
                report.verdict = MergeImpact::NoTreeEffect;
                report.evidence = format!("result tree {result_tree} equals base tree {base_tree}");
            } else {
                report.verdict = MergeImpact::TreeEffect;
                report.evidence =
                    format!("result tree {result_tree} differs from base tree {base_tree}");
            }
        }
        Some(0) => {
            "merge-tree-empty".clone_into(&mut report.evidence);
            report.reason =
                Some("git merge-tree returned no tree object; cannot model the merge".to_owned());
        }
        Some(1) => {
            "merge-tree".clone_into(&mut report.method);
            "merge-conflict".clone_into(&mut report.evidence);
            report.reason = Some(format!("merging {head} into {base} produces conflicts"));
        }
        Some(code) => {
            "merge-tree-failed".clone_into(&mut report.evidence);
            report.reason = Some(format!(
                "git merge-tree --write-tree failed with exit {code}: {} (Git >= 2.38 is required)",
                stderr.trim()
            ));
        }
        None => {
            "merge-tree-terminated".clone_into(&mut report.evidence);
            report.reason = Some("git merge-tree terminated without an exit status".to_owned());
        }
    }
    report
}

/// Resolves the base revision for a PR.
///
/// The verified `baseRefOid` reported by `GitHub` is authoritative. A stale
/// `origin/<base>` is only a labelled fallback when `GitHub` supplied no OID; if
/// the OID is known but its object is missing locally, no ref is substituted.
#[must_use]
pub fn pr_base_rev(
    root: &Path,
    base_ref_oid: &str,
    base: &str,
) -> (Option<String>, Vec<String>, bool) {
    if !base_ref_oid.is_empty() {
        if rev_exists(root, base_ref_oid) {
            return (Some(base_ref_oid.to_owned()), Vec::new(), true);
        }
        return (
            None,
            vec![format!(
                "current base commit {base_ref_oid} is not present locally; run `git fetch origin {base}` and retry"
            )],
            false,
        );
    }
    for candidate in base_refs(base) {
        if rev_exists(root, &candidate) {
            return (
                Some(candidate),
                vec![
                    "PR base commit OID unavailable; using the local base ref, which may be stale"
                        .to_owned(),
                ],
                false,
            );
        }
    }
    (
        None,
        vec![format!(
            "no local ref resolves base `{base}`; run `git fetch origin {base}`"
        )],
        false,
    )
}

/// Merge impact for a PR, using the verified remote base when `GitHub` reports it.
#[must_use]
pub fn merge_impact_for_pr(
    root: &Path,
    base_ref_oid: &str,
    base: &str,
    head: &str,
) -> MergeImpactReport {
    let (rev, warnings, verified) = pr_base_rev(root, base_ref_oid, base);
    if let Some(rev) = rev {
        let mut report = merge_impact(root, &rev, head);
        base.clone_into(&mut report.base);
        report.base_verified = verified;
        let mut combined = warnings;
        combined.extend(report.warnings);
        report.warnings = combined;
        if !verified {
            // A stale or unverifiable base cannot prove the merge leaves the
            // current tree unchanged.
            report.verdict = MergeImpact::Unknown;
            report.evidence = format!("unverified-base; {}", report.evidence);
            report.reason = Some(
                "the current remote base could not be verified locally; refusing to prove a no-tree-effect"
                    .to_owned(),
            );
        }
        report
    } else {
        let mut report = MergeImpactReport::unknown(
            base,
            head,
            "the current remote base could not be verified locally",
        );
        report.warnings = warnings;
        report
    }
}
