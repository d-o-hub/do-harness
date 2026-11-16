//! `do-harness cleanup` — reclaim the build output of disposable worktrees.
//!
//! Two questions decide every candidate, and both are answered
//! deterministically:
//!
//! 1. *Was this branch merged?* — [`proof`], which prefers the merged pull
//!    request and falls back to ancestry or tree equality only when no pull
//!    request is open.
//! 2. *Is anything using this tree right now?* — [`guard`], which refuses a
//!    dirty worktree, a locked or prunable registry entry, and a build tree
//!    with a held Cargo lock or a live compiler process inside it.
//!
//! Only a candidate that passes both is reclaimed, and the reclamation is
//! `cargo clean --target-dir` followed by `git worktree remove` — Cargo's own
//! API for its own directory, and git's own API for its own registry.

mod discover;
mod guard;
mod proof;
mod report;
#[cfg(test)]
#[path = "tests.rs"]
mod tests;

use std::path::{Path, PathBuf};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use anyhow::{Result, bail};

use crate::exit::UsageError;
use crate::report::Format;

pub use report::{Candidate, Report, Verdict};

/// Why a candidate was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reason {
    /// The worktree has uncommitted or untracked files.
    Dirty,
    /// `git worktree lock` pinned the worktree.
    Locked,
    /// The worktree directory is already gone.
    Prunable,
    /// A Cargo build lock or a live compiler process uses the build tree.
    InUse,
    /// No liveness probe was available and `--optimistic` was not passed.
    InUseUnknown,
    /// No merged pull request and no local proof.
    Unmerged,
    /// A merged pull request exists for the branch name, but not for this head.
    ProofStale,
    /// The branch has an open pull request.
    OpenPr,
    /// The worktree's `HEAD` is detached.
    Detached,
    /// `gh` could not answer, so the merge is unproven.
    Unproven,
    /// `origin/main` is unresolvable or its fetch is too old.
    StaleBase,
    /// No build tree exists at the resolved location.
    NoTarget,
    /// The build tree is redirected outside the worktree without `--include-shared`.
    RedirectedUnknown,
    /// The path is the main worktree, `.do-harness/`, a symlink, or outside the contract.
    Protected,
    /// `--keep` retained this eligible tree.
    Kept,
    /// The deletion command failed.
    DeleteFailed,
    /// The local worktree registry could not be read.
    RegistryUnreadable,
}

impl Reason {
    /// Wire spelling for the report.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Dirty => "dirty",
            Self::Locked => "locked",
            Self::Prunable => "prunable",
            Self::InUse => "in_use",
            Self::InUseUnknown => "in_use_unknown",
            Self::Unmerged => "unmerged",
            Self::ProofStale => "proof_stale",
            Self::OpenPr => "open_pr",
            Self::Detached => "detached",
            Self::Unproven => "unproven",
            Self::StaleBase => "stale_base",
            Self::NoTarget => "no_target",
            Self::RedirectedUnknown => "redirected_unknown",
            Self::Protected => "protected",
            Self::Kept => "kept",
            Self::DeleteFailed => "delete_failed",
            Self::RegistryUnreadable => "registry_unreadable",
        }
    }

    /// Parses a wire spelling.
    #[must_use]
    pub fn from_code(code: &str) -> Option<Self> {
        ALL_REASONS
            .into_iter()
            .find(|reason| reason.as_str() == code)
    }

    /// Whether this refusal is an error (exit 1) rather than a safety skip.
    ///
    /// A safety skip means the command worked and correctly declined to touch
    /// something. An error means the command could not answer the question it
    /// was asked, and the operator must act.
    #[must_use]
    pub fn is_error_class(self) -> bool {
        matches!(
            self,
            Self::Unproven
                | Self::StaleBase
                | Self::RegistryUnreadable
                | Self::InUseUnknown
                | Self::DeleteFailed
        )
    }
}

/// Every reason, for code parsing.
const ALL_REASONS: [Reason; 17] = [
    Reason::Dirty,
    Reason::Locked,
    Reason::Prunable,
    Reason::InUse,
    Reason::InUseUnknown,
    Reason::Unmerged,
    Reason::ProofStale,
    Reason::OpenPr,
    Reason::Detached,
    Reason::Unproven,
    Reason::StaleBase,
    Reason::NoTarget,
    Reason::RedirectedUnknown,
    Reason::Protected,
    Reason::Kept,
    Reason::DeleteFailed,
    Reason::RegistryUnreadable,
];

/// Options for one cleanup run.
#[derive(Debug, Clone)]
pub struct Options {
    /// Report the plan and reclaim nothing.
    pub dry_run: bool,
    /// Text or JSON.
    pub format: Format,
    /// Retain this many most-recently-used eligible build trees.
    pub keep: Option<usize>,
    /// Restrict to these worktrees.
    pub worktrees: Vec<PathBuf>,
    /// Extra build trees to consider for every candidate.
    pub target_dirs: Vec<PathBuf>,
    /// Permit build trees outside their worktree.
    pub include_shared: bool,
    /// Reclaim even when the merge proof is unavailable.
    pub optimistic: bool,
}

/// Runs one cleanup pass.
///
/// # Errors
///
/// Returns a usage error (exit 2) when the repository or a named worktree
/// cannot be read, and a verification failure (exit 1) when any candidate was
/// skipped for an error-class reason or a deletion failed.
pub fn run(root: &Path, options: &Options) -> Result<()> {
    let registry =
        discover::registry(root).map_err(|error| anyhow::Error::from(UsageError::wrap(error)))?;
    let records = proof::registry(root).ok();
    let base = discover::base(root).ok();
    let now = now_unix();
    let stale_base = base.as_ref().is_some_and(|base| !proof::fresh(base, now));
    let context = Context {
        root,
        registry: &registry,
        records: records.as_deref(),
        base: base.as_ref(),
        stale_base,
        options,
    };

    let mut candidates: Vec<Candidate> = context
        .select(&options.worktrees)?
        .into_iter()
        .map(|entry| context.inspect(entry))
        .collect();
    apply_keep(&mut candidates, options.keep);

    let reclaimed = if options.dry_run {
        0
    } else {
        reclaim(root, &mut candidates)
    };
    let report = Report {
        schema_version: report::SCHEMA_VERSION,
        root: root.display().to_string(),
        generated_at_unix: now,
        dry_run: options.dry_run,
        origin_main: base.as_ref().map(|base| base.oid.clone()),
        origin_main_fetched_at_unix: base.as_ref().and_then(|base| base.fetched_at_unix),
        candidates,
        reclaimed_bytes: reclaimed,
    };
    match options.format {
        Format::Json => println!("{}", serde_json::to_string_pretty(&report)?),
        Format::Text => print!("{}", report.render_text()),
    }
    if report.has_errors() {
        bail!(
            "cleanup could not prove or reclaim every candidate; see the report for the error-class reasons"
        );
    }
    Ok(())
}

/// Everything one pass needs, so candidate inspection takes one argument.
struct Context<'a> {
    root: &'a Path,
    registry: &'a discover::Registry,
    records: Option<&'a [proof::Record]>,
    base: Option<&'a discover::Base>,
    stale_base: bool,
    options: &'a Options,
}

impl Context<'_> {
    /// Restricts the registry to the requested worktrees, in registry order.
    fn select<'b>(&'b self, requested: &[PathBuf]) -> Result<Vec<&'b discover::Entry>> {
        if requested.is_empty() {
            return Ok(self.registry.linked().collect());
        }
        requested
            .iter()
            .map(|path| {
                self.registry.find(path).ok_or_else(|| {
                    anyhow::Error::from(UsageError::msg(format!(
                        "{} is not a registered worktree of {}",
                        path.display(),
                        self.registry.main.display()
                    )))
                })
            })
            .collect()
    }

    /// Builds one candidate: refusal checks, target resolution, guards, proof.
    fn inspect(&self, entry: &discover::Entry) -> Candidate {
        let started = Instant::now();
        let mut candidate = Candidate {
            worktree: entry.path.display().to_string(),
            branch: entry.branch.clone(),
            head: entry.head.clone(),
            target_dir: None,
            target_bytes: None,
            probe: None,
            merge_proof: proof::MergeProof::unproven(),
            verdict: Verdict::Eligible,
            reason: None,
            reclaimed_bytes: None,
            duration_ms: 0,
        };
        if let Some(reason) = self.refusal(entry) {
            candidate.reason = Some(reason.as_str());
        } else {
            self.classify(entry, &mut candidate);
        }
        if candidate.reason.is_some() {
            candidate.verdict = Verdict::Refused;
        }
        candidate.duration_ms = duration_ms(started);
        candidate
    }

    /// The reason a candidate cannot even be inspected, if any.
    fn refusal(&self, entry: &discover::Entry) -> Option<Reason> {
        if entry.path == self.registry.main {
            return Some(Reason::Protected);
        }
        if self.records.is_none() {
            return Some(Reason::RegistryUnreadable);
        }
        if self.base.is_none() || (self.stale_base && !self.options.optimistic) {
            return Some(Reason::StaleBase);
        }
        None
    }

    /// Resolves the target, runs the guards, then proves the merge.
    fn classify(&self, entry: &discover::Entry, candidate: &mut Candidate) {
        let record = self
            .records
            .and_then(|records| records.iter().find(|record| record.worktree == entry.path));
        match proof::target(
            entry,
            record,
            &self.options.target_dirs,
            self.options.include_shared,
        ) {
            proof::TargetOutcome::Target(target) => {
                candidate.target_bytes = Some(guard::dir_size(&target));
                candidate.target_dir = Some(target.display().to_string());
                let guarded = guard::inspect(
                    entry,
                    &self.registry.main,
                    Some(&target),
                    self.options.optimistic,
                );
                candidate.probe = Some(guarded.probe.as_str());
                if let Some(reason) = guarded.refusal {
                    candidate.reason = Some(reason.as_str());
                } else {
                    let explicit = self
                        .options
                        .worktrees
                        .iter()
                        .any(|path| path == &entry.path);
                    match proof::prove(self.root, entry, explicit, self.options.optimistic) {
                        proof::Proof::Merged(merge_proof) => candidate.merge_proof = merge_proof,
                        proof::Proof::Refused(reason) => {
                            candidate.reason = Some(reason.as_str());
                        }
                    }
                }
            }
            proof::TargetOutcome::Missing => candidate.reason = Some(Reason::NoTarget.as_str()),
            proof::TargetOutcome::Redirected => {
                candidate.reason = Some(Reason::RedirectedUnknown.as_str());
            }
        }
    }
}

/// Retains the `keep` most recently used eligible build trees.
fn apply_keep(candidates: &mut [Candidate], keep: Option<usize>) {
    let Some(keep) = keep else {
        return;
    };
    let mut eligible: Vec<(usize, SystemTime)> = candidates
        .iter()
        .enumerate()
        .filter(|(_, candidate)| candidate.verdict == Verdict::Eligible)
        .map(|(index, candidate)| {
            let modified = candidate
                .target_dir
                .as_ref()
                .and_then(|dir| std::fs::metadata(dir).ok()?.modified().ok())
                .unwrap_or(UNIX_EPOCH);
            (index, modified)
        })
        .collect();
    eligible.sort_by_key(|left| std::cmp::Reverse(left.1));
    for (index, _) in eligible.into_iter().skip(keep) {
        candidates[index].verdict = Verdict::Refused;
        candidates[index].reason = Some(Reason::Kept.as_str());
    }
}

/// Reclaims every eligible candidate, returning the bytes removed.
fn reclaim(root: &Path, candidates: &mut [Candidate]) -> u64 {
    let mut reclaimed: u64 = 0;
    let mut removed_any = false;
    for candidate in candidates.iter_mut() {
        if candidate.verdict != Verdict::Eligible {
            continue;
        }
        let started = Instant::now();
        if let Ok(bytes) = remove(root, candidate) {
            candidate.verdict = Verdict::Cleaned;
            candidate.reclaimed_bytes = Some(bytes);
            reclaimed = reclaimed.saturating_add(bytes);
            removed_any = true;
        } else {
            candidate.verdict = Verdict::Failed;
            candidate.reason = Some(Reason::DeleteFailed.as_str());
        }
        candidate.duration_ms = candidate.duration_ms.saturating_add(duration_ms(started));
    }
    if removed_any {
        let _ = discover::git(root, &["worktree", "prune"]);
    }
    reclaimed
}

/// Cleans one build tree, then removes the worktree.
fn remove(root: &Path, candidate: &Candidate) -> Result<u64> {
    let worktree = PathBuf::from(&candidate.worktree);
    let target = candidate
        .target_dir
        .as_ref()
        .map(PathBuf::from)
        .ok_or_else(|| anyhow::anyhow!("candidate has no target directory"))?;
    let bytes = candidate.target_bytes.unwrap_or(0);
    clean(&worktree, &target)?;
    discover::git(root, &["worktree", "remove", &candidate.worktree])?;
    Ok(bytes)
}

/// Runs `cargo clean --target-dir`, then removes an out-of-worktree tree.
fn clean(worktree: &Path, target: &Path) -> Result<()> {
    let manifest = worktree.join("Cargo.toml");
    let mut command = std::process::Command::new("cargo");
    command.arg("clean").arg("--target-dir").arg(target);
    if manifest.is_file() {
        command.arg("--manifest-path").arg(&manifest);
    }
    let output = command
        .current_dir(worktree)
        .output()
        .map_err(|error| anyhow::anyhow!("failed to run cargo clean: {error}"))?;
    if !output.status.success() {
        bail!(
            "cargo clean failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    if !target.starts_with(worktree) && target.exists() {
        std::fs::remove_dir_all(target)?;
    }
    Ok(())
}

/// Milliseconds elapsed, saturating on absurd durations.
fn duration_ms(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)
}

/// UTC epoch seconds.
fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| {
            i64::try_from(duration.as_secs()).unwrap_or(i64::MAX)
        })
}
