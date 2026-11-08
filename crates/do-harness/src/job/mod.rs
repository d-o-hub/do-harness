//! `do-harness job`: bounded unattended execution with resumable outcomes.
//!
//! A job is explicitly invoked with a spec. It runs an external executor in a
//! disposable workspace at a pinned revision, within finite attempt and
//! time budgets, and completes only when the repository's own strict,
//! recorded verification produces fresh green evidence. An executor's exit
//! status or “done” claim is never proof.
//!
//! Stops are typed, durable blocked outcomes: budget and time exhaustion,
//! repeated or alternating failures, executor timeouts and spawn failures,
//! malformed result declarations, unavailable authorization or capability,
//! and ambiguous requirements. Resuming continues the remaining budget and
//! never renews it, and a declared repair is applied at most once per job.

use std::path::Path;

use anyhow::{Context, Result};

use crate::exit::UsageError;
use crate::report::Format;

mod executor;
mod gate;
mod progress;
mod receipt;
mod repair;
mod runner;
mod spec;
mod workspace;

use receipt::{JobReceipt, JobState};

/// Characters of the resolved commit shown in a receipt line.
const REVISION_PREFIX_CHARS: usize = 12;

/// Runs or resumes one unattended job from `spec_path`.
///
/// # Errors
///
/// Returns a usage error for an invalid spec or a conflicting resume, and a
/// verification failure when the job ends blocked.
pub async fn run(
    root: &Path,
    spec_path: &Path,
    resume: bool,
    dry_run: bool,
    format: Format,
) -> Result<()> {
    let (spec, spec_text) = spec::JobSpec::load(spec_path)
        .map_err(|error| anyhow::Error::from(UsageError::wrap(error)))?;
    let outcome = runner::run(runner::Options {
        root,
        spec: &spec,
        spec_text: &spec_text,
        resume,
        dry_run,
    })
    .await?;
    match outcome {
        runner::RunOutcome::Planned(plan) => {
            print_plan(&plan, format)?;
            Ok(())
        }
        runner::RunOutcome::Finished(receipt) => {
            print_receipt(&receipt, format)?;
            if receipt.state == JobState::Blocked {
                let reason = receipt
                    .blocked
                    .as_ref()
                    .context("blocked job without a typed reason")?;
                anyhow::bail!(
                    "job {} blocked ({}): {}",
                    receipt.job_id,
                    reason.code.as_str(),
                    reason.detail
                );
            }
            Ok(())
        }
    }
}

/// Reports durable job receipts without running anything.
///
/// # Errors
///
/// Returns a usage error when `id` names no receipt, or when the jobs
/// directory cannot be read.
pub fn status(root: &Path, id: Option<&str>, format: Format) -> Result<()> {
    let receipts = match id {
        Some(id) => vec![
            JobReceipt::load(root, id)
                .map_err(|error| anyhow::Error::from(UsageError::wrap(error)))?,
        ],
        None => JobReceipt::list(root)?,
    };
    match format {
        Format::Json => {
            println!("{}", serde_json::to_string_pretty(&receipts)?);
        }
        Format::Text => {
            if receipts.is_empty() {
                println!("no unattended jobs recorded");
            }
            for receipt in &receipts {
                println!("{}", receipt_line(receipt));
                if let Some(reason) = &receipt.blocked {
                    println!("  detail: {}", reason.detail);
                    println!("  resume: {}", reason.resume);
                }
            }
        }
    }
    Ok(())
}

/// One-line receipt summary for text output.
fn receipt_line(receipt: &JobReceipt) -> String {
    let state = match &receipt.blocked {
        Some(reason) => format!("blocked ({})", reason.code.as_str()),
        None => "verified".to_owned(),
    };
    format!(
        "{} {state} attempts {}/{} revision {} workspace {}",
        receipt.job_id,
        receipt.attempts_used,
        receipt.attempts_limit,
        &receipt.resolved_revision[..receipt.resolved_revision.len().min(REVISION_PREFIX_CHARS)],
        if receipt.workspace_removed {
            "removed"
        } else {
            receipt.workspace.as_str()
        }
    )
}

/// Renders a dry-run plan.
fn print_plan(plan: &runner::Plan, format: Format) -> Result<()> {
    match format {
        Format::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "status": "planned",
                    "job_id": plan.job_id,
                    "resolved_revision": plan.resolved_revision,
                    "executor": plan.executor,
                    "signal_set": plan.signal_set,
                    "isolation": plan.isolation,
                    "attempts": plan.attempts,
                    "deadline_seconds": plan.deadline_seconds,
                }))?
            );
        }
        Format::Text => {
            println!("would start {} (dry run)", plan.job_id);
            println!("  revision: {}", plan.resolved_revision);
            println!("  executor: {}", plan.executor);
            println!("  isolation (declared): {}", plan.isolation);
            println!("  gate: signal set {}", plan.signal_set);
            println!(
                "  budget: {} attempts, {}s deadline",
                plan.attempts, plan.deadline_seconds
            );
        }
    }
    Ok(())
}

/// Renders a finished receipt.
fn print_receipt(receipt: &JobReceipt, format: Format) -> Result<()> {
    match format {
        Format::Json => println!("{}", serde_json::to_string_pretty(receipt)?),
        Format::Text => {
            println!("{}", receipt_line(receipt));
            if let Some(reason) = &receipt.blocked {
                println!("  code: {}", reason.code.as_str());
                println!("  detail: {}", reason.detail);
                println!("  resume: {}", reason.resume);
            }
        }
    }
    Ok(())
}
