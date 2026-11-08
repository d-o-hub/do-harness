//! Versioned audit and resume state for the PR sweep.
//!
//! The state file lives under the resolved git metadata directory via
//! `git rev-parse --git-path pr-triage/sweep-state-v1.json` — never a hard
//! `.git` path, because worktrees and unusual gitdirs resolve differently.
//!
//! Receipts are never cached approvals: every re-entry reloads comments,
//! checks, and policy from scratch. The state only records what *happened*
//! so an interrupted run can reconcile an uncertain prior mutation before it
//! is repeated (for example an update-branch request that may or may not have
//! landed).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::pr::gh;

/// Relative state path inside the git metadata directory.
pub const STATE_GIT_PATH: &str = "pr-triage/sweep-state-v1.json";

/// Schema version of the sweep state file.
pub const STATE_SCHEMA_VERSION: u32 = 1;

/// Per-PR audit record.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PrAudit {
    /// Terminal outcome code when this PR completed a run.
    pub outcome: Option<String>,
    /// Head SHA observed when the audit was written.
    pub head_sha: Option<String>,
    /// Base SHA observed when the audit was written.
    pub base_sha: Option<String>,
    /// Merge commit SHA after a direct merge.
    pub merge_sha: Option<String>,
    /// Post-merge main verification: `pass`, `regression`, or `unknown`.
    pub post_merge: Option<String>,
    /// Whether an armed auto-merge request was disarmed in a prior run.
    pub disarmed_auto_merge: bool,
    /// Whether a branch update was requested in a prior run.
    pub update_requested: bool,
    /// Head SHA pinned when the last update was requested.
    pub update_expected_head: Option<String>,
}

/// Mode record for one sweep run.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModeRecord {
    /// Read-only inventory and report.
    pub dry_run: bool,
    /// Authorize owned/bot branch updates and auto-merge disarm.
    pub execute: bool,
    /// Also authorize eligible merges and closures.
    pub merge: bool,
}

fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| i64::try_from(duration.as_secs()).unwrap_or(0))
}

/// The persisted sweep state.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SweepStateV1 {
    /// Schema version.
    pub schema_version: u32,
    /// Mode of the run that wrote this state.
    pub mode: ModeRecord,
    /// UTC epoch seconds when the run started.
    pub created_at_unix: i64,
    /// Per-PR audit records.
    pub prs: BTreeMap<u64, PrAudit>,
}

impl SweepStateV1 {
    /// A fresh state for `mode`.
    #[must_use]
    pub fn fresh(mode: ModeRecord) -> Self {
        Self {
            schema_version: STATE_SCHEMA_VERSION,
            mode,
            created_at_unix: now_unix(),
            prs: BTreeMap::new(),
        }
    }

    /// Loads the state file, starting fresh when absent or malformed.
    #[must_use]
    pub fn load(path: &Path) -> Self {
        let Ok(text) = std::fs::read_to_string(path) else {
            return Self::fresh(ModeRecord {
                dry_run: true,
                execute: false,
                merge: false,
            });
        };
        serde_json::from_str(&text).unwrap_or_else(|_| {
            Self::fresh(ModeRecord {
                dry_run: true,
                execute: false,
                merge: false,
            })
        })
    }

    /// Persists the state (parent directories are created).
    ///
    /// # Errors
    ///
    /// Returns an error when the file cannot be written.
    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let text = serde_json::to_string_pretty(self)?;
        std::fs::write(path, text)?;
        Ok(())
    }

    /// Reconciles an uncertain prior branch-update request.
    ///
    /// If the recorded expected head is still the current head *and* the PR
    /// is still behind, the prior update never landed, so the request can be
    /// repeated. If the head moved, the update (or author work) landed and the
    /// next run revalidates from scratch; either way the flag is cleared so a
    /// repeat never assumes stale history.
    pub fn reconcile(&mut self, root: &Path, open_numbers: &[u64]) {
        for number in open_numbers {
            let Some(audit) = self.prs.get_mut(number) else {
                continue;
            };
            if !audit.update_requested {
                continue;
            }
            if gh::readiness_view(root, *number).is_err() {
                // Keep the uncertain flag until the state can be observed.
                continue;
            }
            // Whether the prior request landed or not, the next run re-reads
            // the head and revalidates from scratch; never assume stale
            // history.
            audit.update_requested = false;
            audit.update_expected_head = None;
        }
    }
}

/// Resolves the state file path under the git metadata directory.
///
/// Falls back to `<root>/.git/pr-triage/…` when the repo is not a git
/// worktree (or git is unavailable); the sweep itself still requires gh.
#[must_use]
pub fn state_path(root: &Path) -> PathBuf {
    let output = crate::changes::git_command(root)
        .args(["rev-parse", "--git-path", STATE_GIT_PATH])
        .output();
    if let Ok(output) = output {
        if output.status.success() {
            let raw = String::from_utf8_lossy(&output.stdout);
            let trimmed = raw.trim();
            if !trimmed.is_empty() {
                let path = PathBuf::from(trimmed);
                if path.is_absolute() {
                    return path;
                }
                return root.join(path);
            }
        }
    }
    root.join(".git").join(STATE_GIT_PATH)
}
