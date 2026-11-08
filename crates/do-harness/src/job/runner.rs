//! Bounded unattended job orchestration.
//!
//! One job is: a disposable workspace at a pinned revision, an external
//! executor with finite attempts and time, deterministic repairs declared by
//! project policy, and the repository's own strict verification as the only
//! proof of completion. Every stop is a typed blocked outcome that preserves
//! its revisions, budget, and resume information.

use std::collections::BTreeSet;
use std::fs::File;
use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use fs2::FileExt;

use crate::config;
use crate::exit::UsageError;

use super::executor::{self, Invocation};
use super::gate::{self, GateOutcome};
use super::progress::{self, ProgressTracker, ProgressVerdict};
use super::receipt::{
    AttemptRecord, BlockedCode, BlockedReason, ExecutorReportStatus, JobReceipt, JobState,
};
use super::repair;
use super::spec::JobSpec;
use super::workspace;

/// Lock file preventing two runs of one job.
const LOCK_FILE: &str = "lock";

/// What a `job run` produced without finishing a job.
pub struct Plan {
    /// Deterministic job id.
    pub job_id: String,
    /// Commit the requested revision resolved to.
    pub resolved_revision: String,
    /// Executor identity.
    pub executor: String,
    /// Signal set gating completion.
    pub signal_set: String,
    /// Declared isolation mechanism.
    pub isolation: String,
    /// Attempts the job may use.
    pub attempts: u32,
    /// Whole-job budget in seconds.
    pub deadline_seconds: u64,
}

/// Result of a `job run` invocation.
pub enum RunOutcome {
    /// Dry run: what would have started, with no side effects.
    Planned(Plan),
    /// The job reached a durable state (verified, or already terminal).
    Finished(Box<JobReceipt>),
}

/// Inputs to one `job run`.
pub struct Options<'a> {
    /// Repository root.
    pub root: &'a Path,
    /// Validated spec.
    pub spec: &'a JobSpec,
    /// Raw spec text, snapshotted beside the receipt for audit.
    pub spec_text: &'a str,
    /// Continue an interrupted job instead of refusing an existing one.
    pub resume: bool,
    /// Plan only; create nothing and run nothing.
    pub dry_run: bool,
}

/// Runs or resumes one job.
///
/// # Errors
///
/// Returns a usage error for an unusable spec, revision, workspace, or
/// conflicting resume, and an ordinary error when durable state cannot be
/// written.
#[allow(clippy::too_many_lines)]
pub async fn run(options: Options<'_>) -> Result<RunOutcome> {
    let spec = options.spec;
    let job_id = spec.job_id()?;
    let paths = JobReceipt::paths(options.root, &job_id);
    let resolved = workspace::resolve_revision(options.root, &spec.revision)?;
    if options.dry_run {
        return Ok(RunOutcome::Planned(Plan {
            job_id,
            resolved_revision: resolved,
            executor: spec.executor.program().to_owned(),
            signal_set: spec.signal_set.clone(),
            isolation: spec.isolation.mechanism.clone(),
            attempts: spec.attempts,
            deadline_seconds: spec.deadline_seconds,
        }));
    }
    if !options.resume && paths.receipt.exists() {
        return Err(existing_job_error(&job_id).into());
    }
    let _lock = acquire_lock(&paths.dir)?;
    let mut receipt = if paths.receipt.exists() {
        let receipt = JobReceipt::load(options.root, &job_id)?;
        if receipt.spec_digest != spec.digest()? {
            return Err(UsageError::msg("job receipt belongs to a different spec").into());
        }
        receipt
    } else {
        if options.resume {
            return Err(UsageError::msg(format!(
                "no job {job_id} to resume at {}",
                paths.receipt.display()
            ))
            .into());
        }
        std::fs::create_dir_all(&paths.dir)
            .with_context(|| format!("failed to create {}", paths.dir.display()))?;
        std::fs::write(&paths.spec_snapshot, options.spec_text)
            .with_context(|| format!("failed to write {}", paths.spec_snapshot.display()))?;
        new_receipt(spec, &job_id, &resolved, &paths.workspace, options.root)?
    };
    if receipt.state != JobState::Running {
        return Ok(RunOutcome::Finished(Box::new(receipt)));
    }
    if now() >= receipt.deadline_at {
        return block(
            &mut receipt,
            options.root,
            BlockedCode::DeadlineExhausted,
            "the whole-job time limit expired before an attempt could start",
        );
    }
    workspace::prepare(options.root, &paths.workspace, &resolved)?;
    receipt.save(options.root)?;
    let cfg = config::load(&paths.workspace, None).await?;
    let mut tracker = ProgressTracker::from_attempts(&receipt.attempts);
    loop {
        if now() >= receipt.deadline_at {
            return block(
                &mut receipt,
                options.root,
                BlockedCode::DeadlineExhausted,
                "the whole-job time limit expired",
            );
        }
        if receipt.attempts_remaining() == 0 {
            return block(
                &mut receipt,
                options.root,
                BlockedCode::AttemptsExhausted,
                "the attempt budget was spent without verified completion",
            );
        }
        receipt.attempts_used += 1;
        receipt.save(options.root)?;
        let attempt = receipt.attempts_used;
        let started_at = now();
        let result_path = paths.dir.join(format!("result-{attempt}.json"));
        let _ = std::fs::remove_file(&result_path);
        let observation = executor::run(&Invocation {
            spec: &spec.executor,
            workspace: &paths.workspace,
            job_id: &job_id,
            attempt,
            result_path: &result_path,
            timeout: Duration::from_secs(spec.executor_timeout_seconds).min(remaining(&receipt)),
        });
        let mut outcome = gate::run(&paths.workspace, &spec.signal_set, remaining(&receipt));
        let mut repairs_applied = Vec::new();
        if !outcome.green {
            let budget = remaining(&receipt);
            repairs_applied = repair::apply(
                &cfg.repairs,
                outcome.verification.as_ref(),
                &mut receipt,
                &paths.workspace,
                options.root,
                budget,
            );
            if !repairs_applied.is_empty() {
                outcome = gate::run(&paths.workspace, &spec.signal_set, remaining(&receipt));
            }
        }
        let (signature, failure_set) = signature(&outcome);
        let mut record = AttemptRecord {
            attempt,
            started_at,
            finished_at: now(),
            executor: observation,
            repairs_applied,
            verification: outcome.verification.clone(),
            failure_signature: None,
            failure_set: Vec::new(),
            blocked: None,
        };
        if outcome.green {
            receipt.state = JobState::Verified;
            receipt.blocked = None;
            receipt.workspace_removed = workspace::remove(options.root, &paths.workspace);
            receipt.attempts.push(record);
            receipt.save(options.root)?;
            return Ok(RunOutcome::Finished(Box::new(receipt)));
        }
        record.failure_signature = Some(signature.clone());
        record.failure_set = failure_set.iter().cloned().collect();
        let verdict = tracker.evaluate(&signature, &failure_set);
        if let Some(code) = terminal_executor_code(&record) {
            let reason = BlockedReason::new(code, detail(&record, &outcome), resume_text(code));
            record.blocked = Some(reason.clone());
            receipt.attempts.push(record);
            return block_with(&mut receipt, options.root, Some(reason));
        }
        if matches!(
            verdict,
            ProgressVerdict::Repeated | ProgressVerdict::Regressed
        ) {
            let reason = BlockedReason::new(
                BlockedCode::NoProgress,
                format!("attempt {attempt} did not reduce the failure set"),
                resume_text(BlockedCode::NoProgress),
            );
            record.blocked = Some(reason.clone());
            receipt.attempts.push(record);
            return block_with(&mut receipt, options.root, Some(reason));
        }
        if receipt.attempts_remaining() == 0 {
            let code = if record.executor.report_error.is_some() {
                BlockedCode::InvalidExecutorOutput
            } else {
                BlockedCode::VerificationFailed
            };
            let reason = BlockedReason::new(code, detail(&record, &outcome), resume_text(code));
            record.blocked = Some(reason.clone());
            receipt.attempts.push(record);
            return block_with(&mut receipt, options.root, Some(reason));
        }
        receipt.attempts.push(record);
        receipt.save(options.root)?;
    }
}

/// Builds the new-job receipt.
fn new_receipt(
    spec: &JobSpec,
    job_id: &str,
    resolved: &str,
    workspace_path: &Path,
    root: &Path,
) -> Result<JobReceipt> {
    let started_at = now();
    let deadline_at =
        started_at.saturating_add(i64::try_from(spec.deadline_seconds).unwrap_or(i64::MAX));
    let workspace = workspace_path.strip_prefix(root).map_or_else(
        |_| workspace_path.display().to_string(),
        |path| path.display().to_string(),
    );
    Ok(JobReceipt {
        schema_version: super::receipt::RECEIPT_SCHEMA_VERSION,
        job_id: job_id.to_owned(),
        spec_digest: spec.digest()?,
        task: super::receipt::bounded(&spec.task),
        revision: spec.revision.clone(),
        resolved_revision: resolved.to_owned(),
        signal_set: spec.signal_set.clone(),
        isolation: spec.isolation.mechanism.clone(),
        executor: spec.executor.program().to_owned(),
        accounting: format!("{:?}", spec.executor.accounting).to_lowercase(),
        attempts_limit: spec.attempts,
        attempts_used: 0,
        started_at,
        deadline_at,
        state: JobState::Running,
        blocked: None,
        attempts: Vec::new(),
        applied_repairs: Vec::new(),
        workspace,
        workspace_removed: false,
    })
}

/// Fails closed when the executor could not run, hung, or declared a block.
fn terminal_executor_code(record: &AttemptRecord) -> Option<BlockedCode> {
    if record.executor.spawn_error.is_some() {
        return Some(BlockedCode::ExecutorUnavailable);
    }
    if record.executor.timed_out {
        return Some(BlockedCode::ExecutorTimeout);
    }
    let report = record.executor.report.as_ref()?;
    if report.status != ExecutorReportStatus::Blocked {
        return None;
    }
    let code = report.blocked.as_ref().map(|claim| claim.code.as_str())?;
    Some(match code {
        "authorization-unavailable" => BlockedCode::AuthorizationUnavailable,
        "ambiguous-requirements" => BlockedCode::AmbiguousRequirements,
        _ => BlockedCode::UnsupportedCapability,
    })
}

/// Bounded diagnostic for a stop.
fn detail(record: &AttemptRecord, outcome: &GateOutcome) -> String {
    record
        .executor
        .report_error
        .clone()
        .or_else(|| record.executor.spawn_error.clone())
        .or_else(|| outcome.diagnostic.clone())
        .unwrap_or_else(|| "the attempt did not produce verifiable completion".to_owned())
}

/// Operator guidance that never implies renewed budget or broader access.
fn resume_text(code: BlockedCode) -> String {
    match code {
        BlockedCode::AttemptsExhausted | BlockedCode::DeadlineExhausted => {
            "raise the spec's attempts or deadline-seconds to start a new job; resume never renews budget"
        }
        BlockedCode::NoProgress => {
            "change the task or spec so the executor can make progress; resuming does not renew budget"
        }
        BlockedCode::AuthorizationUnavailable => {
            "supply the credentials the spec declares in allowed-env; this CLI never requests broader access"
        }
        BlockedCode::UnsupportedCapability => {
            "run the job where the declared executor capability exists"
        }
        BlockedCode::AmbiguousRequirements => {
            "resolve the requirement with an operator, then start a new job"
        }
        BlockedCode::VerificationFailed | BlockedCode::InvalidExecutorOutput => {
            "inspect the preserved workspace and attempt receipts, then start a new job"
        }
        BlockedCode::ExecutorTimeout | BlockedCode::ExecutorUnavailable => {
            "fix the executor invocation or its per-attempt budget, then start a new job"
        }
    }
    .to_owned()
}

/// Signature over sensor names and machine reasons, never diagnostic text.
fn signature(outcome: &GateOutcome) -> (String, BTreeSet<String>) {
    outcome.verification.as_ref().map_or_else(
        || progress::signature(&[], &[], None, None),
        |record| {
            progress::signature(
                &record.failed,
                &record.unrunnable,
                Some(&record.state),
                Some(&record.reason),
            )
        },
    )
}

/// Remaining whole-job budget.
fn remaining(receipt: &JobReceipt) -> Duration {
    Duration::from_secs(
        u64::try_from(receipt.deadline_at.saturating_sub(now()).max(0)).unwrap_or(0),
    )
}

/// Marks the receipt blocked and persists it.
fn block(
    receipt: &mut JobReceipt,
    root: &Path,
    code: BlockedCode,
    detail: &str,
) -> Result<RunOutcome> {
    let reason = BlockedReason::new(code, detail, resume_text(code));
    block_with(receipt, root, Some(reason))
}

/// Marks the receipt blocked with an explicit reason.
fn block_with(
    receipt: &mut JobReceipt,
    root: &Path,
    reason: Option<BlockedReason>,
) -> Result<RunOutcome> {
    receipt.state = JobState::Blocked;
    receipt.blocked = reason;
    receipt.save(root)?;
    Ok(RunOutcome::Finished(Box::new(receipt.clone())))
}

/// Locks a job directory for the duration of the run.
fn acquire_lock(dir: &Path) -> Result<File> {
    std::fs::create_dir_all(dir).with_context(|| format!("failed to create {}", dir.display()))?;
    let file = File::create(dir.join(LOCK_FILE))
        .with_context(|| format!("failed to open the job lock in {}", dir.display()))?;
    file.try_lock_exclusive().map_err(|_| {
        UsageError::msg(format!(
            "job directory {} is in use by another run",
            dir.display()
        ))
    })?;
    Ok(file)
}

/// Usage error for a job that already exists.
fn existing_job_error(job_id: &str) -> UsageError {
    UsageError::msg(format!(
        "job {job_id} already exists; pass --resume to continue its remaining budget, \
         or change the spec to start a new job"
    ))
}

/// Current Unix seconds.
fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| {
            i64::try_from(elapsed.as_secs()).unwrap_or(i64::MAX)
        })
}
