//! `do-harness sync`: refresh `origin/main` and reconcile the local `main`.
//!
//! This is the deterministic form of the manual "switch to main, pull, validate"
//! sequence. It fetches with the same refspec as the harness
//! `latest-main.sh` helper, fast-forwards only a clean branch that is strictly
//! behind, and never stashes, rebases, or resets unless the caller passes
//! `--reset-diverged`. Refusals are exit code 1 and leave the checkout alone;
//! git and remote failures are exit code 2.

mod git;

use std::path::Path;

use anyhow::Result;

use self::git::{
    counts, create_backup, current_branch, ensure_remote, ensure_work_tree, fast_forward, fetch,
    head, is_clean, local_branch_sha, local_commits, ls_remote, object_exists, reset_hard,
    rev_parse, short, switch_to_main,
};
use serde::Serialize;

use crate::report::{Format, json_pretty};
use crate::status::{StatusReport, status_report};
use crate::{CliError, config};

/// Remote whose `main` branch is the sync target.
const REMOTE: &str = "origin";
/// Branch the command reconciles.
const BRANCH: &str = "main";
/// Signal set reported by `--status`: the ordinary completion gate.
const VERIFICATION_SET: &str = "verification";
/// Bound on local commit subjects printed from a diverged branch.
const MAX_LOCAL_COMMITS: usize = 10;
/// Short object-id prefix used in backup branch names.
const SHORT_SHA_LEN: usize = 7;

/// Outcome of a sync attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SyncState {
    /// Local `main` already matches `origin/main`.
    UpToDate,
    /// Local `main` was fast-forwarded (or reset) onto `origin/main`.
    Synced,
    /// The checkout was switched to `main`; `fast_forwarded` says whether it moved.
    Switched,
    /// `origin/main` was fetched, but the current branch was left untouched.
    Fetched,
    /// `--read-only` (or `--dry-run`): the remote was queried without fetching.
    ReadOnly,
    /// A dirty worktree blocked a switch or fast-forward.
    RefusedDirty,
    /// Local `main` has commits that are not on `origin/main`.
    RefusedDiverged,
}

impl SyncState {
    /// Whether the state is a refusal (exit code 1).
    #[must_use]
    pub fn refused(self) -> bool {
        matches!(self, Self::RefusedDirty | Self::RefusedDiverged)
    }
}

/// Machine-readable sync report (`--format json` contract).
#[allow(clippy::struct_excessive_bools)] // independent facts: clean, switched, fast-forwarded, reset, aligned
#[derive(Debug, Clone, Serialize)]
pub struct SyncReport {
    /// Outcome of the attempt.
    pub state: SyncState,
    /// Remote that was queried.
    pub remote: &'static str,
    /// Branch that was reconciled.
    pub branch: &'static str,
    /// Branch checked out before any switch; absent when detached.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub previous_branch: Option<String>,
    /// Commit advertised at `origin/main` by the remote.
    pub advertised: String,
    /// Current `HEAD` after the attempt.
    pub head: String,
    /// Commits on local `main` not on `origin/main`; absent when untracked.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ahead: Option<usize>,
    /// Commits on `origin/main` not on local `main`; absent when untracked.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub behind: Option<usize>,
    /// Whether the worktree was clean before the attempt.
    pub clean: bool,
    /// Whether the checkout was switched to `main`.
    pub switched: bool,
    /// Whether `main` moved forward onto `origin/main`.
    pub fast_forwarded: bool,
    /// Whether a diverged `main` was backed up and reset.
    pub reset: bool,
    /// Whether `HEAD` equals the advertised `origin/main` commit.
    pub aligned: bool,
    /// Backup branch created by `--reset-diverged`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backup_ref: Option<String>,
    /// Local commit subjects from a diverged branch, bounded.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub local_commits: Vec<String>,
    /// Evidence freshness embedded by `--status`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verification: Option<StatusReport>,
}

/// Options for [`run`].
#[allow(clippy::struct_excessive_bools)] // four independent CLI switches
#[derive(Debug, Clone, Copy)]
pub struct SyncOpts {
    /// Check out `main` when the current branch is clean.
    pub switch: bool,
    /// Query the advertised ref without fetching or switching.
    pub read_only: bool,
    /// Back up and hard-reset a diverged `main`.
    pub reset_diverged: bool,
    /// Embed verification evidence freshness.
    pub status: bool,
}

/// Runs `do-harness sync`.
///
/// # Errors
///
/// Returns [`CliError::Usage`] for git, remote, or config problems (exit 2) and
/// [`CliError::Verify`] for a dirty or diverged refusal (exit 1).
pub async fn run(
    root: &Path,
    config_path: Option<&Path>,
    opts: SyncOpts,
    dry_run: bool,
    format: Format,
) -> Result<(), CliError> {
    let read_only = opts.read_only || dry_run;
    let mut report = execute(root, opts, read_only).map_err(CliError::Usage)?;
    if opts.status {
        report.verification = Some(
            verification_status(root, config_path)
                .await
                .map_err(CliError::Usage)?,
        );
    }
    print_report(&report, format).map_err(CliError::Usage)?;
    if report.state.refused() {
        return Err(CliError::Verify(anyhow::anyhow!(
            "{}",
            refusal_hint(&report, opts)
        )));
    }
    Ok(())
}

/// Performs the sync and returns the typed report without printing.
fn execute(root: &Path, opts: SyncOpts, read_only: bool) -> Result<SyncReport> {
    ensure_work_tree(root)?;
    ensure_remote(root)?;

    let previous = current_branch(root)?;
    let clean = is_clean(root)?;
    let local_main = local_branch_sha(root)?;

    let advertised = if read_only {
        ls_remote(root)?
    } else {
        fetch(root)?;
        rev_parse(root, &format!("refs/remotes/{REMOTE}/{BRANCH}"))?
    };

    let head_before = head(root)?;
    let mut report = SyncReport {
        state: SyncState::UpToDate,
        remote: REMOTE,
        branch: BRANCH,
        previous_branch: previous.clone(),
        advertised: advertised.clone(),
        head: head_before.clone(),
        ahead: None,
        behind: None,
        clean,
        switched: false,
        fast_forwarded: false,
        reset: false,
        aligned: head_before == advertised,
        backup_ref: None,
        local_commits: Vec::new(),
        verification: None,
    };

    if read_only {
        report.state = SyncState::ReadOnly;
        if local_main.is_some() && object_exists(root, &advertised)? {
            let (ahead, behind) = counts(root, &format!("refs/heads/{BRANCH}"), &advertised)?;
            report.ahead = Some(ahead);
            report.behind = Some(behind);
        }
        return Ok(report);
    }

    if let Some(sha) = &local_main {
        let (ahead, behind) = counts(root, sha, &format!("refs/remotes/{REMOTE}/{BRANCH}"))?;
        report.ahead = Some(ahead);
        report.behind = Some(behind);
    }

    let on_main = previous.as_deref() == Some(BRANCH);
    if !on_main {
        // `--reset-diverged` is explicit recovery, so it also brings the
        // checkout to `main`; a plain default run only fetches.
        if !opts.switch && !opts.reset_diverged {
            report.state = SyncState::Fetched;
            return Ok(report);
        }
        if !clean {
            report.state = SyncState::RefusedDirty;
            return Ok(report);
        }
        // Refuse a diverged `main` *before* switching so a refusal leaves the
        // checkout exactly as it was: the caller either approves the reset or
        // resolves the local commits themselves.
        if !opts.reset_diverged && report.ahead.unwrap_or(0) > 0 {
            report.local_commits = local_commits(root)?;
            report.state = SyncState::RefusedDiverged;
            return Ok(report);
        }
        switch_to_main(root, local_main.is_some())?;
        report.switched = true;
    }

    reconcile(root, opts, &mut report)?;
    report.head = head(root)?;
    report.aligned = report.head == report.advertised;
    Ok(report)
}

/// Fast-forwards, refuses, or resets after the checkout is on `main`.
fn reconcile(root: &Path, opts: SyncOpts, report: &mut SyncReport) -> Result<()> {
    // A branch created from `origin/main` during the switch is already equal;
    // `counts` would fail because no local ref existed before the switch.
    if report.switched && report.ahead.is_none() {
        report.ahead = Some(0);
        report.behind = Some(0);
    }
    let ahead = report.ahead.unwrap_or(0);
    let behind = report.behind.unwrap_or(0);

    if ahead > 0 {
        if opts.reset_diverged && report.clean {
            let sha = head(root)?;
            report.backup_ref = Some(create_backup(root, &sha)?);
            reset_hard(root)?;
            report.reset = true;
            report.state = SyncState::Synced;
        } else if opts.reset_diverged {
            report.state = SyncState::RefusedDirty;
        } else {
            report.local_commits = local_commits(root)?;
            report.state = SyncState::RefusedDiverged;
        }
    } else if behind > 0 {
        if report.clean {
            fast_forward(root)?;
            report.fast_forwarded = true;
            report.state = if report.switched {
                SyncState::Switched
            } else {
                SyncState::Synced
            };
        } else {
            report.state = SyncState::RefusedDirty;
        }
    } else {
        report.state = if report.switched {
            SyncState::Switched
        } else {
            SyncState::UpToDate
        };
    }
    Ok(())
}

/// Embeds evidence freshness for the ordinary verification set.
async fn verification_status(root: &Path, config_path: Option<&Path>) -> Result<StatusReport> {
    let (cfg, config_bytes) = config::load_raw(root, config_path).await?;
    status_report(
        root,
        &cfg,
        config_bytes.as_deref(),
        Some(VERIFICATION_SET),
        None,
    )
}

/// Prints the report in the requested format.
fn print_report(report: &SyncReport, format: Format) -> Result<()> {
    match format {
        Format::Text => print_text(report),
        Format::Json => println!("{}", json_pretty(report)?),
    }
    Ok(())
}

/// Prints the human-readable report.
fn print_text(report: &SyncReport) {
    match report.state {
        SyncState::UpToDate => println!(
            "sync: {}/{} is up to date at {}",
            report.remote,
            report.branch,
            short(&report.advertised)
        ),
        SyncState::Synced if report.reset => println!(
            "sync: reset {} to {}/{} ({})",
            report.branch,
            report.remote,
            report.branch,
            short(&report.advertised)
        ),
        SyncState::Synced => println!(
            "sync: fast-forwarded {} to {}/{} ({})",
            report.branch,
            report.remote,
            report.branch,
            short(&report.advertised)
        ),
        SyncState::Switched => println!(
            "sync: switched to {} at {}",
            report.branch,
            short(&report.advertised)
        ),
        SyncState::Fetched => println!(
            "sync: fetched {}/{} at {}; current branch left untouched",
            report.remote,
            report.branch,
            short(&report.advertised)
        ),
        SyncState::ReadOnly => println!(
            "sync: {}/{} advertised at {} (read-only, not fetched)",
            report.remote,
            report.branch,
            short(&report.advertised)
        ),
        SyncState::RefusedDirty => {
            println!("sync: refused — worktree has uncommitted changes");
        }
        SyncState::RefusedDiverged => println!(
            "sync: refused — {} has {} local commit(s) not on {}/{}",
            report.branch,
            report.ahead.unwrap_or(0),
            report.remote,
            report.branch
        ),
    }
    if report.switched {
        if let Some(previous) = &report.previous_branch {
            println!("  switched from {previous}");
        }
    }
    println!("  branch {} @ {}", report.branch, short(&report.head));
    println!(
        "  worktree {}",
        if report.clean { "clean" } else { "dirty" }
    );
    for commit in &report.local_commits {
        println!("  local: {commit}");
    }
    if let Some(backup) = &report.backup_ref {
        println!("  backup: {backup}");
    }
    if let Some(verification) = &report.verification {
        println!(
            "  verification: {:?} ({})",
            verification.state, verification.reason
        );
    }
}

/// Actionable message for a refusal (printed to stderr after the report).
fn refusal_hint(report: &SyncReport, opts: SyncOpts) -> String {
    match report.state {
        SyncState::RefusedDirty => {
            let recovery = if opts.reset_diverged {
                " --reset-diverged"
            } else if report.switched {
                ""
            } else {
                " --switch"
            };
            format!(
                "worktree is dirty; commit or stash the changes, then rerun `do-harness sync{recovery}`"
            )
        }
        SyncState::RefusedDiverged => format!(
            "{} has {} local commit(s) not on {}/{}; inspect them, then pass --reset-diverged to back up and reset",
            report.branch,
            report.ahead.unwrap_or(0),
            report.remote,
            report.branch
        ),
        _ => String::from("sync did not complete"),
    }
}

#[cfg(test)]
#[path = "sync_tests.rs"]
mod sync_tests;
