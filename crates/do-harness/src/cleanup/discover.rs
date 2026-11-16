//! Discovery of linked worktrees and the fetched remote default branch.
//!
//! `git worktree list --porcelain` is the only supported source: it is stable
//! enough to parse, and it reports the two states a cleanup must never touch —
//! `locked` and `prunable` — which a directory walk cannot see.

use std::path::{Path, PathBuf};
use std::process::Output;
use std::time::UNIX_EPOCH;

use anyhow::{Context, Result, bail};

use crate::changes::git_command;

/// One entry of `git worktree list --porcelain`.
#[derive(Debug, Clone)]
pub struct Entry {
    /// Absolute path of the working tree.
    pub path: PathBuf,
    /// Commit the worktree's `HEAD` points at.
    pub head: String,
    /// Short branch name when the worktree is on a branch.
    pub branch: Option<String>,
    /// Whether `HEAD` is detached.
    pub detached: bool,
    /// Whether the entry is a bare repository.
    pub bare: bool,
    /// Lock reason when `git worktree lock` pinned the entry.
    pub locked: Option<String>,
    /// Prune reason when the directory is already missing.
    pub prunable: Option<String>,
}

/// The repository's worktree registry, main worktree first.
#[derive(Debug, Clone)]
pub struct Registry {
    /// The main worktree (the one that owns `.git/`).
    pub main: PathBuf,
    /// Every registered worktree, in `git worktree list` order.
    pub entries: Vec<Entry>,
}

impl Registry {
    /// Linked worktrees only: the main worktree is never a cleanup candidate.
    pub fn linked(&self) -> impl Iterator<Item = &Entry> {
        self.entries.iter().filter(|entry| entry.path != self.main)
    }

    /// Finds a registered worktree by path, accepting either spelling.
    #[must_use]
    pub fn find(&self, path: &Path) -> Option<&Entry> {
        self.entries.iter().find(|entry| {
            entry.path == path
                || entry
                    .path
                    .canonicalize()
                    .is_ok_and(|canonical| canonical == path)
        })
    }
}

/// Reads the worktree registry.
///
/// # Errors
///
/// Returns an error when `git worktree list` cannot run, exits non-zero, or
/// reports no main worktree. An unreadable registry is never treated as an
/// empty one: that would make a destructive command claim there is nothing to
/// do while it is blind.
pub fn registry(root: &Path) -> Result<Registry> {
    let output = git(root, &["worktree", "list", "--porcelain"])?;
    let text = String::from_utf8_lossy(&output.stdout);
    let entries = parse(&text)?;
    let main = entries
        .first()
        .map(|entry| entry.path.clone())
        .context("git worktree list reported no worktrees")?;
    Ok(Registry { main, entries })
}

/// Parses porcelain worktree output.
///
/// # Errors
///
/// Returns an error when a block has no `worktree` line or no `HEAD`, which
/// means the output is not the format this parser claims to read.
pub fn parse(text: &str) -> Result<Vec<Entry>> {
    let mut entries = Vec::new();
    let mut current: Option<Entry> = None;
    for line in text.lines().chain(std::iter::once("")) {
        if line.is_empty() {
            if let Some(entry) = current.take() {
                if entry.head.is_empty() {
                    bail!("worktree {} has no HEAD line", entry.path.display());
                }
                entries.push(entry);
            }
            continue;
        }
        if let Some(path) = line.strip_prefix("worktree ") {
            current = Some(Entry {
                path: PathBuf::from(path),
                head: String::new(),
                branch: None,
                detached: false,
                bare: false,
                locked: None,
                prunable: None,
            });
            continue;
        }
        let entry = current
            .as_mut()
            .with_context(|| format!("porcelain line outside a worktree block: {line}"))?;
        apply(entry, line);
    }
    Ok(entries)
}

/// Applies one porcelain attribute to the current entry.
fn apply(entry: &mut Entry, line: &str) {
    if let Some(head) = line.strip_prefix("HEAD ") {
        head.clone_into(&mut entry.head);
    } else if let Some(branch) = line.strip_prefix("branch ") {
        entry.branch = Some(
            branch
                .strip_prefix("refs/heads/")
                .unwrap_or(branch)
                .to_owned(),
        );
    } else if line == "detached" {
        entry.detached = true;
    } else if line == "bare" {
        entry.bare = true;
    } else if let Some(reason) = line.strip_prefix("locked") {
        entry.locked = Some(reason.trim().to_owned());
    } else if let Some(reason) = line.strip_prefix("prunable") {
        entry.prunable = Some(reason.trim().to_owned());
    }
}

/// The fetched base a local merge proof compares against.
#[derive(Debug, Clone)]
pub struct Base {
    /// Remote-tracking reference the proof compares against, e.g. `origin/main`.
    pub reference: String,
    /// Commit id that reference currently names.
    pub oid: String,
    /// When the remote-tracking ref last moved, when that is knowable.
    pub fetched_at_unix: Option<i64>,
}

/// Remote-tracking refs to consider, in order, when `origin/HEAD` is unset.
const BASE_CANDIDATES: [&str; 2] = ["main", "master"];

/// Resolves the remote default branch and its fetch age.
///
/// The base is whatever the remote calls its default branch — resolved locally
/// from `origin/HEAD` so no network call is needed — never a hardcoded `main`.
/// A hardcoded name makes the command useless in every repository that still
/// uses `master`, where the base would never resolve and every candidate would
/// be reported as an error.
///
/// # Errors
///
/// Returns an error when no candidate base ref resolves: a local proof against
/// an unresolvable base is not a proof.
pub fn base(root: &Path) -> Result<Base> {
    let reference = base_reference(root)?;
    let output = git(
        root,
        &["rev-parse", "--verify", &format!("{reference}^{{commit}}")],
    )?;
    let oid = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if oid.is_empty() {
        bail!("{reference} did not resolve to a commit");
    }
    let fetched_at_unix = fetch_time(root, &format!("refs/remotes/{reference}"));
    Ok(Base {
        reference,
        oid,
        fetched_at_unix,
    })
}
/// The remote-tracking ref naming the default branch.
///
/// `origin/HEAD` is the authoritative local answer; the candidate list is the
/// documented fallback for a clone that never recorded it.
fn base_reference(root: &Path) -> Result<String> {
    if let Ok(output) = git(
        root,
        &["symbolic-ref", "--short", "refs/remotes/origin/HEAD"],
    ) {
        let name = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        if !name.is_empty()
            && git(
                root,
                &["rev-parse", "--verify", &format!("{name}^{{commit}}")],
            )
            .is_ok()
        {
            return Ok(name);
        }
    }
    for branch in BASE_CANDIDATES {
        let reference = format!("origin/{branch}");
        if git(
            root,
            &["rev-parse", "--verify", &format!("{reference}^{{commit}}")],
        )
        .is_ok()
        {
            return Ok(reference);
        }
    }
    bail!(
        "no remote default branch resolved: tried origin/HEAD, {}",
        BASE_CANDIDATES
            .iter()
            .map(|branch| format!("origin/{branch}"))
            .collect::<Vec<_>>()
            .join(", ")
    )
}

/// Best-effort fetch timestamp for the remote-tracking ref.
///
/// `FETCH_HEAD` is written by `git fetch`; when it is absent the loose
/// remote-tracking ref or `packed-refs` carries the last write time. `None`
/// means the age is unknowable, which callers treat as stale unless the
/// operator opts in.
fn fetch_time(root: &Path, reference: &str) -> Option<i64> {
    let git_dir = git(root, &["rev-parse", "--absolute-git-dir"])
        .ok()
        .map(|output| PathBuf::from(String::from_utf8_lossy(&output.stdout).trim().to_owned()))?;
    [
        git_dir.join("FETCH_HEAD"),
        git_dir.join(reference),
        git_dir.join("packed-refs"),
    ]
    .iter()
    .find_map(|path| {
        let modified = std::fs::metadata(path).ok()?.modified().ok()?;
        let seconds = modified.duration_since(UNIX_EPOCH).ok()?.as_secs();
        i64::try_from(seconds).ok()
    })
}

/// Runs `git <args>` in `root`, failing on spawn errors and non-zero exits.
///
/// # Errors
///
/// Returns an error carrying `git`'s stderr when the command fails.
pub fn git(root: &Path, args: &[&str]) -> Result<Output> {
    let output = git_command(root)
        .args(args)
        .output()
        .with_context(|| format!("failed to run git {}", args.join(" ")))?;
    if !output.status.success() {
        bail!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(output)
}
