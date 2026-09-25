//! CI failure and cancellation explanation with local sensor mapping (#243).
//!
//! When CI fails or cancels:
//! - Dissects workflow jobs, categorizing `cancelled` separately from `failure`
//!   with exact `gh run rerun` reproduction guidance.
//! - Maps failed jobs back to the closest local sensor and the exact command
//!   that reproduces the failure, preferring the `FAIL <name>` markers the
//!   harness itself prints in the failed step's log.

mod gh;
mod map;

use std::path::Path;

use anyhow::Result;
use serde::{Deserialize, Serialize};

pub use map::parse_run_id;

use crate::CliError;
use crate::config;
use crate::report::Format;

/// Structured CI explanation report.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CiExplainReport {
    pub run_id: u64,
    pub workflow_name: String,
    pub status: String,
    pub conclusion: Option<String>,
    pub html_url: String,
    pub jobs: Vec<JobExplanation>,
    pub summary: CiSummary,
}

/// Detailed analysis of a single CI job.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobExplanation {
    pub id: u64,
    pub name: String,
    pub conclusion: Option<String>,
    pub html_url: String,
    pub classification: JobClassification,
    pub failed_step: Option<String>,
    pub failure_reason: Option<String>,
    pub local_sensor: Option<String>,
    pub local_repro_command: Option<String>,
    pub rerun_command: Option<String>,
}

/// Classification of a CI job outcome.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum JobClassification {
    Passed,
    Cancelled,
    Failed,
    Skipped,
    Pending,
}

/// Aggregate summary of the CI run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CiSummary {
    pub total: usize,
    pub passed: usize,
    pub cancelled: usize,
    pub failed: usize,
    pub pending: usize,
    pub skipped: usize,
    pub recommended_action: String,
}

/// Explains workflow run `raw_run_id` and prints it in `format`.
///
/// # Errors
///
/// Returns a usage error for an unparseable run ID and a verify error when the
/// run cannot be read from `GitHub`.
pub async fn run(root: &Path, raw_run_id: &str, format: Format) -> Result<(), CliError> {
    let run_id = parse_run_id(raw_run_id).map_err(CliError::Usage)?;
    let report = explain(root, run_id).await.map_err(CliError::Verify)?;
    match format {
        Format::Json => println!(
            "{}",
            serde_json::to_string_pretty(&report).map_err(|e| CliError::Verify(e.into()))?
        ),
        Format::Text => print_text(&report),
    }
    Ok(())
}

/// Dissects workflow `run_id` and maps failures to local sensors.
pub async fn explain(root: &Path, run_id: u64) -> Result<CiExplainReport> {
    let run_meta = gh::fetch_run_metadata(root, run_id)?;
    let raw_jobs = gh::fetch_jobs(root, run_id)?;

    let cfg = config::load(root, None).await.ok();
    let empty_sensors = Vec::new();
    let sensors = cfg
        .as_ref()
        .map_or(&empty_sensors[..], config::Config::effective_sensors);

    let mut jobs = Vec::new();
    let mut counts = JobCounts::default();
    for item in raw_jobs {
        let classification = classify(&item);
        counts.observe(classification);

        let failed = classification == JobClassification::Failed;
        let failed_step = if failed { failing_step(&item) } else { None };
        let annotations = if failed {
            gh::fetch_annotations(root, item.id)
        } else {
            Vec::new()
        };
        let signals = if failed {
            gh::failed_step_signals(root, item.id)
        } else {
            gh::FailedLogSignals::default()
        };
        let failure_reason = if failed {
            failure_reason(&signals, &annotations)
        } else {
            None
        };

        let (local_sensor, local_repro_command) = if failed {
            map::map_sensor(
                &item.name,
                failed_step.as_deref(),
                failure_reason.as_deref(),
                &signals,
                sensors,
            )
        } else {
            (None, None)
        };

        let rerun_command = (classification == JobClassification::Cancelled)
            .then(|| format!("gh run rerun {run_id} --job {}", item.id));

        jobs.push(JobExplanation {
            id: item.id,
            name: item.name,
            conclusion: item.conclusion,
            html_url: item.html_url,
            classification,
            failed_step,
            failure_reason,
            local_sensor,
            local_repro_command,
            rerun_command,
        });
    }

    let recommended_action = recommend(run_id, &jobs, &counts);
    Ok(CiExplainReport {
        run_id,
        workflow_name: run_meta.name,
        status: run_meta.status,
        conclusion: run_meta.conclusion,
        html_url: run_meta.html_url,
        jobs,
        summary: CiSummary {
            total: counts.total,
            passed: counts.passed,
            cancelled: counts.cancelled,
            failed: counts.failed,
            pending: counts.pending,
            skipped: counts.skipped,
            recommended_action,
        },
    })
}

/// Job tally while walking the run.
#[derive(Default)]
struct JobCounts {
    total: usize,
    passed: usize,
    cancelled: usize,
    failed: usize,
    pending: usize,
    skipped: usize,
}

impl JobCounts {
    fn observe(&mut self, classification: JobClassification) {
        self.total += 1;
        match classification {
            JobClassification::Passed => self.passed += 1,
            JobClassification::Cancelled => self.cancelled += 1,
            JobClassification::Failed => self.failed += 1,
            JobClassification::Pending => self.pending += 1,
            JobClassification::Skipped => self.skipped += 1,
        }
    }
}

/// Buckets a job outcome, keeping `cancelled` distinct from `failure`.
fn classify(job: &gh::JobItem) -> JobClassification {
    if job.status != "completed" {
        return JobClassification::Pending;
    }
    match job.conclusion.as_deref() {
        Some("success") => JobClassification::Passed,
        Some("skipped" | "neutral") => JobClassification::Skipped,
        Some("cancelled") => JobClassification::Cancelled,
        _ => JobClassification::Failed,
    }
}

/// The first step that reported failure.
fn failing_step(job: &gh::JobItem) -> Option<String> {
    job.steps
        .iter()
        .find(|step| step.conclusion.as_deref() == Some("failure"))
        .map(|step| step.name.clone())
}

/// Failure text: the newest compiler/test error from the log, else the
/// annotation, else nothing (a bare non-zero exit the log did not explain).
fn failure_reason(
    signals: &gh::FailedLogSignals,
    annotations: &[gh::AnnotationItem],
) -> Option<String> {
    if let Some(line) = signals.error_lines.last() {
        return Some(line.clone());
    }
    annotations
        .iter()
        .find(|a| a.annotation_level.as_deref() == Some("failure"))
        .or_else(|| annotations.first())
        .and_then(|a| {
            let message = a.message.as_deref()?.trim();
            match a.path.as_deref() {
                Some(path) if !path.is_empty() => Some(format!("{message} ({path})")),
                _ => Some(message.to_owned()),
            }
        })
}

/// The single action this run calls for.
fn recommend(run_id: u64, jobs: &[JobExplanation], counts: &JobCounts) -> String {
    if counts.failed > 0 {
        if let Some(cmd) = jobs
            .iter()
            .find(|job| job.classification == JobClassification::Failed)
            .and_then(|job| job.local_repro_command.as_deref())
        {
            return format!("reproduce locally with: {cmd}");
        }
        return "inspect the failed job logs; no local sensor matched".to_owned();
    }
    if counts.cancelled > 0 {
        return format!("gh run rerun {run_id}");
    }
    if counts.pending > 0 {
        return "run is still in progress; re-run this command when it settles".to_owned();
    }
    "no action needed; all jobs passed".to_owned()
}

fn print_text(report: &CiExplainReport) {
    let conclusion = report.conclusion.as_deref().unwrap_or("in_progress");
    println!(
        "run {} ({}): {} ({})",
        report.run_id, report.workflow_name, report.status, conclusion
    );
    if !report.html_url.is_empty() {
        println!("  url: {}", report.html_url);
    }
    println!();
    let s = &report.summary;
    println!(
        "jobs ({} total: {} passed, {} failed, {} cancelled, {} pending, {} skipped):",
        s.total, s.passed, s.failed, s.cancelled, s.pending, s.skipped
    );

    for job in &report.jobs {
        match job.classification {
            JobClassification::Failed => {
                println!("  FAILED: {}", job.name);
                if let Some(step) = &job.failed_step {
                    println!("    failed step: {step}");
                }
                if let Some(reason) = &job.failure_reason {
                    println!("    reason: {reason}");
                }
                match (&job.local_sensor, &job.local_repro_command) {
                    (Some(sensor), Some(cmd)) => {
                        println!("    local sensor: {sensor}");
                        println!("    reproduce: {cmd}");
                    }
                    _ => println!("    no local sensor matched; inspect the job log"),
                }
            }
            JobClassification::Cancelled => {
                println!("  CANCELLED: {}", job.name);
                if let Some(cmd) = &job.rerun_command {
                    println!("    rerun: {cmd}");
                }
                println!(
                    "    advice: cancelled jobs usually stop on an external cause (superseded \
                     push, concurrency group, runner loss), not a code defect; rerun before \
                     changing code"
                );
            }
            JobClassification::Pending => println!("  PENDING: {}", job.name),
            JobClassification::Passed | JobClassification::Skipped => {}
        }
    }
    println!();
    println!(
        "recommended action:\n  {}",
        report.summary.recommended_action
    );
}
