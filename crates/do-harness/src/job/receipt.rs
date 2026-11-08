//! Durable job receipts: the resume, idempotency, and blocked-outcome contract.
//!
//! Every job owns `.do-harness/jobs/<id>/job.json`. The receipt is written
//! before an attempt starts (so an interrupted job is discoverable) and after
//! it ends (so a resumed job continues its remaining budget instead of
//! renewing it). Receipts are local state and are never committed.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::redaction::Redactor;

/// Receipt schema version understood by this binary.
pub const RECEIPT_SCHEMA_VERSION: u32 = 1;
/// Directory (relative to the workspace root) holding job state.
pub const JOBS_DIR: &str = ".do-harness/jobs";
/// Receipt file name inside a job directory.
pub const RECEIPT_FILE: &str = "job.json";
/// Spec snapshot file name inside a job directory.
pub const SPEC_SNAPSHOT_FILE: &str = "spec.toml";
/// Directory name of the disposable workspace inside a job directory.
pub const WORKSPACE_DIR: &str = "workspace";
/// Longest diagnostic tail kept per attempt.
pub const MAX_DIAGNOSTIC_BYTES: usize = 2048;

/// Lifecycle state of a job.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum JobState {
    /// The job has budget left and may be interrupted or resumed.
    Running,
    /// Fresh, strict verification evidence proves completion.
    Verified,
    /// The job stopped; the reason is typed and preserved.
    Blocked,
}

/// Why a job stopped without proof of completion.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum BlockedCode {
    /// The attempt budget was spent.
    AttemptsExhausted,
    /// The whole-job wall-clock budget was spent.
    DeadlineExhausted,
    /// Attempts stopped making observable progress.
    NoProgress,
    /// The authoritative verification gate is not green.
    VerificationFailed,
    /// The executor program could not be started.
    ExecutorUnavailable,
    /// The executor exceeded its per-attempt budget.
    ExecutorTimeout,
    /// The executor's declared result was missing or malformed.
    InvalidExecutorOutput,
    /// The job lacks credentials or authorization it must not request itself.
    AuthorizationUnavailable,
    /// The executor reported a capability the environment does not support.
    UnsupportedCapability,
    /// The task cannot be completed without an operator decision.
    AmbiguousRequirements,
}

impl BlockedCode {
    /// Machine-readable code as it appears in receipts.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::AttemptsExhausted => "attempts-exhausted",
            Self::DeadlineExhausted => "deadline-exhausted",
            Self::NoProgress => "no-progress",
            Self::VerificationFailed => "verification-failed",
            Self::ExecutorUnavailable => "executor-unavailable",
            Self::ExecutorTimeout => "executor-timeout",
            Self::InvalidExecutorOutput => "invalid-executor-output",
            Self::AuthorizationUnavailable => "authorization-unavailable",
            Self::UnsupportedCapability => "unsupported-capability",
            Self::AmbiguousRequirements => "ambiguous-requirements",
        }
    }
}

/// A typed, actionable blocked outcome.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BlockedReason {
    /// Typed reason code.
    pub code: BlockedCode,
    /// Bounded diagnostic detail (redacted before it is stored).
    pub detail: String,
    /// What an operator can do next without implicitly expanding privileges.
    pub resume: String,
}

impl BlockedReason {
    /// Builds a blocked reason, bounding and redacting the detail.
    #[must_use]
    pub fn new(code: BlockedCode, detail: impl AsRef<str>, resume: impl Into<String>) -> Self {
        Self {
            code,
            detail: bounded(detail.as_ref()),
            resume: resume.into(),
        }
    }
}

/// Executor report declared through `DO_HARNESS_JOB_RESULT`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct ExecutorReport {
    /// `done` or `blocked`; neither is proof of completion.
    pub status: ExecutorReportStatus,
    /// Bounded human summary.
    #[serde(default)]
    pub summary: Option<String>,
    /// Required when `status` is `blocked`.
    #[serde(default)]
    pub blocked: Option<BlockedClaim>,
}

/// Status an executor may declare.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ExecutorReportStatus {
    /// The executor claims it finished the work.
    Done,
    /// The executor cannot continue and names why.
    Blocked,
}

/// Blocked claim an executor may declare.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct BlockedClaim {
    /// One of `authorization-unavailable`, `unsupported-capability`, or
    /// `ambiguous-requirements`.
    pub code: String,
    /// Bounded explanation.
    pub detail: String,
}

/// What the executor process did during one attempt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutorObservation {
    /// Exit code; absent when the process was killed by a signal.
    pub exit_code: Option<i32>,
    /// The per-attempt budget expired and the process tree was killed.
    pub timed_out: bool,
    /// Spawn error, when the program could not start.
    pub spawn_error: Option<String>,
    /// Combined captured byte count.
    pub output_bytes: u64,
    /// Output exceeded the capture budget.
    pub output_truncated: bool,
    /// Redacted, bounded tail of stdout.
    pub stdout_tail: String,
    /// Redacted, bounded tail of stderr.
    pub stderr_tail: String,
    /// Parsed executor report, when one was declared and valid.
    pub report: Option<ExecutorReport>,
    /// Why the declared report is unusable, when it is.
    pub report_error: Option<String>,
}

/// Verification-gate observation for one attempt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VerificationRecord {
    /// Freshness state reported by `status` after the gate ran.
    pub state: String,
    /// Reason reported by `status`.
    pub reason: String,
    /// Sensors that failed, sorted.
    pub failed: Vec<String>,
    /// Sensors that never started, sorted.
    pub unrunnable: Vec<String>,
}

/// One executor attempt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AttemptRecord {
    /// 1-based attempt number across the whole job.
    pub attempt: u32,
    /// Unix seconds when the attempt started.
    pub started_at: i64,
    /// Unix seconds when the attempt ended.
    pub finished_at: i64,
    /// What the executor process did.
    pub executor: ExecutorObservation,
    /// Repair identifiers applied during this attempt.
    pub repairs_applied: Vec<String>,
    /// Authoritative gate result; absent when the gate could not run.
    pub verification: Option<VerificationRecord>,
    /// Progress signature derived from sensor names, never error text.
    pub failure_signature: Option<String>,
    /// Sorted sensor and state identifiers behind the signature.
    #[serde(default)]
    pub failure_set: Vec<String>,
    /// Attempt-local block reason, when the attempt stopped the job.
    pub blocked: Option<BlockedReason>,
}

/// Durable state of one unattended job.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JobReceipt {
    /// Receipt schema version.
    pub schema_version: u32,
    /// Deterministic id derived from the spec digest.
    pub job_id: String,
    /// Digest of the spec that authorizes this job.
    pub spec_digest: String,
    /// Bounded task description.
    pub task: String,
    /// Revision requested by the spec.
    pub revision: String,
    /// Commit the requested revision resolved to.
    pub resolved_revision: String,
    /// Signal set whose evidence gates completion.
    pub signal_set: String,
    /// Declared external isolation mechanism (recorded, never verified here).
    pub isolation: String,
    /// Executor identity (its program).
    pub executor: String,
    /// Declared accounting availability.
    pub accounting: String,
    /// Total attempts allowed across resumes.
    pub attempts_limit: u32,
    /// Attempts consumed so far, including an interrupted in-flight attempt.
    pub attempts_used: u32,
    /// Unix seconds when the job first started.
    pub started_at: i64,
    /// Unix seconds when the whole-job budget expires.
    pub deadline_at: i64,
    /// Lifecycle state.
    pub state: JobState,
    /// Typed block reason; present exactly when `state` is `blocked`.
    pub blocked: Option<BlockedReason>,
    /// Attempt history in order.
    pub attempts: Vec<AttemptRecord>,
    /// Repair identifiers already applied, so resume never applies one twice.
    pub applied_repairs: Vec<String>,
    /// Workspace path relative to the repository root.
    pub workspace: String,
    /// Whether the disposable workspace has been removed.
    pub workspace_removed: bool,
}

impl JobReceipt {
    /// Paths owned by one job.
    #[must_use]
    pub fn paths(root: &Path, job_id: &str) -> JobPaths {
        let dir = root.join(JOBS_DIR).join(job_id);
        JobPaths {
            receipt: dir.join(RECEIPT_FILE),
            spec_snapshot: dir.join(SPEC_SNAPSHOT_FILE),
            workspace: dir.join(WORKSPACE_DIR),
            dir,
        }
    }

    /// Loads a receipt from `root`, failing closed on an inconsistent one.
    ///
    /// # Errors
    ///
    /// Returns an error when the receipt is missing, unreadable, malformed, or
    /// violates a receipt invariant.
    pub fn load(root: &Path, job_id: &str) -> Result<Self> {
        let path = Self::paths(root, job_id).receipt;
        let text = std::fs::read_to_string(&path)
            .with_context(|| format!("failed to read job receipt {}", path.display()))?;
        let receipt: Self = serde_json::from_str(&text)
            .with_context(|| format!("invalid job receipt {}", path.display()))?;
        receipt.check_invariants()?;
        Ok(receipt)
    }

    /// Loads every readable receipt under `root`, sorted by id.
    ///
    /// # Errors
    ///
    /// Returns an error when the jobs directory cannot be listed for a reason
    /// other than absence.
    pub fn list(root: &Path) -> Result<Vec<Self>> {
        let dir = root.join(JOBS_DIR);
        let entries = match std::fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => {
                return Err(error).with_context(|| format!("failed to list {}", dir.display()));
            }
        };
        let mut ids: Vec<String> = entries
            .filter_map(std::result::Result::ok)
            .filter(|entry| entry.path().join(RECEIPT_FILE).is_file())
            .filter_map(|entry| entry.file_name().to_str().map(ToOwned::to_owned))
            .collect();
        ids.sort();
        Ok(ids
            .iter()
            .filter_map(|id| Self::load(root, id).ok())
            .collect())
    }

    /// Persists the receipt atomically.
    ///
    /// # Errors
    ///
    /// Returns an error when the directory or the receipt cannot be written.
    pub fn save(&self, root: &Path) -> Result<()> {
        let paths = Self::paths(root, &self.job_id);
        std::fs::create_dir_all(&paths.dir)
            .with_context(|| format!("failed to create {}", paths.dir.display()))?;
        let text = serde_json::to_string_pretty(self).context("serialize job receipt")?;
        let temporary = paths.dir.join(format!("{RECEIPT_FILE}.tmp"));
        std::fs::write(&temporary, text)
            .with_context(|| format!("failed to write {}", temporary.display()))?;
        std::fs::rename(&temporary, &paths.receipt)
            .with_context(|| format!("failed to replace {}", paths.receipt.display()))
    }

    /// Rejects a receipt that would make resume ambiguous.
    ///
    /// # Errors
    ///
    /// Returns an error naming the violated invariant.
    pub fn check_invariants(&self) -> Result<()> {
        if self.schema_version != RECEIPT_SCHEMA_VERSION {
            anyhow::bail!(
                "job receipt schema {} is not supported (expected {})",
                self.schema_version,
                RECEIPT_SCHEMA_VERSION
            );
        }
        if (self.state == JobState::Blocked) != self.blocked.is_some() {
            anyhow::bail!("job receipt blocked reason disagrees with its state");
        }
        if self.attempts_used > self.attempts_limit {
            anyhow::bail!("job receipt used more attempts than its budget");
        }
        if self.attempts.len() > usize::try_from(self.attempts_used).unwrap_or(usize::MAX) {
            anyhow::bail!("job receipt has more attempt records than attempts used");
        }
        Ok(())
    }

    /// Attempts left without renewing any budget.
    #[must_use]
    pub fn attempts_remaining(&self) -> u32 {
        self.attempts_limit.saturating_sub(self.attempts_used)
    }
}

/// Paths owned by one job.
pub struct JobPaths {
    /// Job directory.
    pub dir: PathBuf,
    /// Receipt file.
    pub receipt: PathBuf,
    /// Snapshot of the authorizing spec.
    pub spec_snapshot: PathBuf,
    /// Disposable workspace.
    pub workspace: PathBuf,
}

/// Truncates and redacts a diagnostic string.
#[must_use]
pub fn bounded(text: &str) -> String {
    let redacted = Redactor::from_environment(&[]).text(text);
    if redacted.len() <= MAX_DIAGNOSTIC_BYTES {
        return redacted;
    }
    let mut end = MAX_DIAGNOSTIC_BYTES;
    while end > 0 && !redacted.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…", &redacted[..end])
}
