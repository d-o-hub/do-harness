//! Read-only `GitHub` Actions access through the `gh` CLI.

use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::{Command, Stdio};

use anyhow::{Context, Result, bail};
use serde::Deserialize;

/// Workflow run header.
#[derive(Debug, Clone, Deserialize)]
pub struct RunMetadata {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub status: String,
    pub conclusion: Option<String>,
    #[serde(default)]
    pub html_url: String,
}

#[derive(Debug, Clone, Deserialize)]
struct JobsResponse {
    #[serde(default)]
    jobs: Vec<JobItem>,
}

/// One workflow job with its steps.
#[derive(Debug, Clone, Deserialize)]
pub struct JobItem {
    pub id: u64,
    pub name: String,
    #[serde(default)]
    pub status: String,
    pub conclusion: Option<String>,
    #[serde(default)]
    pub html_url: String,
    #[serde(default)]
    pub steps: Vec<StepItem>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct StepItem {
    pub name: String,
    pub conclusion: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AnnotationItem {
    pub annotation_level: Option<String>,
    pub message: Option<String>,
    pub path: Option<String>,
}

/// Signal extracted from a failing job's failed-step log, bounded in size.
#[derive(Debug, Default, Clone)]
pub struct FailedLogSignals {
    /// Sensors the harness itself reported as failing (`FAIL  <name>`).
    pub failed_sensors: Vec<String>,
    /// Compiler/test error lines, most recent last.
    pub error_lines: Vec<String>,
}

/// Bound on retained error lines per job (the log itself is streamed).
const ERROR_LINE_LIMIT: usize = 5;
/// Bound on retained harness `FAIL` markers per job.
const FAILED_SENSOR_LIMIT: usize = 5;

impl FailedLogSignals {
    fn observe(&mut self, message: &str) {
        if let Some(name) = failed_sensor_marker(message) {
            if !self.failed_sensors.iter().any(|seen| seen == name)
                && self.failed_sensors.len() < FAILED_SENSOR_LIMIT
            {
                self.failed_sensors.push(name.to_owned());
            }
            return;
        }
        if !is_error_line(message) {
            return;
        }
        if self.error_lines.len() == ERROR_LINE_LIMIT {
            self.error_lines.remove(0);
        }
        self.error_lines.push(message.trim().to_owned());
    }
}

/// Extracts the sensor name from a harness `FAIL  <name>` line.
fn failed_sensor_marker(message: &str) -> Option<&str> {
    let rest = message.trim_start().strip_prefix("FAIL")?;
    let name = rest.trim_start().split([' ', '\t', '(', ':']).next()?;
    if name.is_empty() || name.starts_with(char::is_uppercase) {
        None
    } else {
        Some(name)
    }
}

/// Whether a log line carries a compiler or test failure worth reporting.
fn is_error_line(message: &str) -> bool {
    let trimmed = message.trim();
    trimmed.starts_with("error[")
        || trimmed.starts_with("error:")
        || trimmed.contains("panicked at")
        || trimmed.contains("assertion `left")
        || trimmed.contains("test failed")
        || trimmed.starts_with("Error: ")
}

/// Fetches the run header for `run_id`.
pub fn fetch_run_metadata(root: &Path, run_id: u64) -> Result<RunMetadata> {
    let endpoint = format!("repos/{{owner}}/{{repo}}/actions/runs/{run_id}");
    let output = api(root, &endpoint).with_context(|| format!("gh api run {run_id} failed"))?;
    serde_json::from_slice(&output).context("unexpected run metadata JSON")
}

/// Lists every job of `run_id`, one page of 100 at a time.
pub fn fetch_jobs(root: &Path, run_id: u64) -> Result<Vec<JobItem>> {
    let endpoint = format!("repos/{{owner}}/{{repo}}/actions/runs/{run_id}/jobs?per_page=100");
    let output = api_with(root, &["api", "--paginate", "--slurp", &endpoint])
        .with_context(|| format!("gh api jobs {run_id} failed"))?;
    if let Ok(pages) = serde_json::from_slice::<Vec<JobsResponse>>(&output) {
        return Ok(pages.into_iter().flat_map(|page| page.jobs).collect());
    }
    let single: JobsResponse =
        serde_json::from_slice(&output).context("unexpected run jobs JSON")?;
    Ok(single.jobs)
}

/// Fetches job annotations; unavailable annotations are not an error.
pub fn fetch_annotations(root: &Path, job_id: u64) -> Vec<AnnotationItem> {
    let endpoint = format!("repos/{{owner}}/{{repo}}/check-runs/{job_id}/annotations");
    let Ok(output) = api(root, &endpoint) else {
        return Vec::new();
    };
    serde_json::from_slice(&output).unwrap_or_default()
}

/// Streams the failed steps of `job_id` and extracts bounded failure signals.
///
/// The log is piped and reduced line by line: a failing job can emit tens of
/// megabytes, so only the harness `FAIL` markers and the newest error lines are
/// retained. Log retrieval failure is reported as empty signals, never an error.
pub fn failed_step_signals(root: &Path, job_id: u64) -> FailedLogSignals {
    let mut signals = FailedLogSignals::default();
    let Ok(mut child) = Command::new("gh")
        .current_dir(root)
        .args(["run", "view", "--job", &job_id.to_string(), "--log-failed"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    else {
        return signals;
    };
    if let Some(stdout) = child.stdout.take() {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            signals.observe(log_message(&line));
        }
    }
    let _ = child.wait();
    signals
}

/// Strips the `<job>\t<step>\t<timestamp> ` prefix `gh` adds to log lines.
fn log_message(line: &str) -> &str {
    let mut rest = line;
    for _ in 0..2 {
        match rest.split_once('\t') {
            Some((_, tail)) => rest = tail,
            None => return line,
        }
    }
    match rest.split_once(' ') {
        Some((stamp, message)) if stamp.contains('T') && stamp.ends_with('Z') => message,
        _ => rest,
    }
}

/// Runs `gh api <endpoint>` and returns stdout when it exits 0.
fn api(root: &Path, endpoint: &str) -> Result<Vec<u8>> {
    api_with(root, &["api", endpoint])
}

/// Runs `gh` with `args` and returns stdout when it exits 0.
fn api_with(root: &Path, args: &[&str]) -> Result<Vec<u8>> {
    let output = Command::new("gh")
        .current_dir(root)
        .args(args)
        .output()
        .context("failed to run gh (is the GitHub CLI installed?)")?;
    if !output.status.success() {
        bail!("{}", String::from_utf8_lossy(&output.stderr).trim());
    }
    Ok(output.stdout)
}
