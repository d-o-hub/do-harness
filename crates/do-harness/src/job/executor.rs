//! Bounded external-executor invocation.
//!
//! The executor never inherits this process's environment wholesale: only a
//! fixed base set and the spec's `allowed-env` names reach it, so protected
//! credentials stay separated from untrusted repository code. Its process
//! group is killed when the per-attempt budget expires, so a hung executor
//! cannot outlive the attempt.

use std::collections::VecDeque;
use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};
use std::time::{Duration, Instant};

use crate::redaction::Redactor;
use crate::shell;

use super::receipt::{ExecutorObservation, ExecutorReport};
use super::spec::ExecutorSpec;

/// Combined captured output budget for one executor invocation.
pub const OUTPUT_LIMIT_BYTES: usize = 64 * 1024;
/// Largest declared executor result accepted.
pub const MAX_RESULT_BYTES: usize = 64 * 1024;
/// How often the parent polls the child while waiting.
const POLL: Duration = Duration::from_millis(20);
/// Pipe read chunk.
const CHUNK_BYTES: usize = 8192;

/// Variables the executor always receives so an ordinary program can run.
const BASE_ENV: &[&str] = &[
    "PATH",
    "HOME",
    "TMPDIR",
    "TMP",
    "TEMP",
    "LANG",
    "LC_ALL",
    "TZ",
    "SYSTEMROOT",
    "USERPROFILE",
    "PATHEXT",
    "COMSPEC",
];

/// Everything one invocation needs.
pub struct Invocation<'a> {
    /// Executor identity and configuration.
    pub spec: &'a ExecutorSpec,
    /// Workspace the executor runs in.
    pub workspace: &'a Path,
    /// Job id exported to the executor.
    pub job_id: &'a str,
    /// 1-based attempt number exported to the executor.
    pub attempt: u32,
    /// Path the executor must write its report to.
    pub result_path: &'a Path,
    /// Per-attempt wall-clock budget.
    pub timeout: Duration,
}

/// Runs the executor once, always returning a recordable observation.
#[must_use]
pub fn run(invocation: &Invocation<'_>) -> ExecutorObservation {
    let mut command = Command::new(invocation.spec.program());
    command.args(&invocation.spec.argv[1..]);
    command.current_dir(invocation.workspace);
    command.env_clear();
    for key in BASE_ENV {
        if let Some(value) = std::env::var_os(key) {
            command.env(key, value);
        }
    }
    for key in &invocation.spec.allowed_env {
        if let Some(value) = std::env::var_os(key) {
            command.env(key, value);
        }
    }
    command
        .env("DO_HARNESS_JOB_ID", invocation.job_id)
        .env("DO_HARNESS_JOB_ATTEMPT", invocation.attempt.to_string())
        .env("DO_HARNESS_JOB_WORKSPACE", invocation.workspace)
        .env("DO_HARNESS_JOB_RESULT", invocation.result_path);
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    shell::configure_process_group(&mut command);

    let redactor = Redactor::from_environment(&invocation.spec.allowed_env);
    let bounded = run_bounded(command, invocation.timeout, &redactor);
    let (report, report_error) = read_report(
        invocation.result_path,
        invocation.spec.result_required,
        &redactor,
    );
    ExecutorObservation {
        exit_code: bounded.exit_code,
        timed_out: bounded.timed_out,
        spawn_error: bounded.spawn_error,
        output_bytes: bounded.output_bytes,
        output_truncated: bounded.output_truncated,
        stdout_tail: bounded.stdout,
        stderr_tail: bounded.stderr,
        report,
        report_error,
    }
}

/// What a bounded child process produced.
pub(super) struct BoundedOutput {
    /// Exit code; absent when killed by a signal.
    pub exit_code: Option<i32>,
    /// The wall-clock budget expired and the process tree was killed.
    pub timed_out: bool,
    /// Why the program could not be started or waited for.
    pub spawn_error: Option<String>,
    /// Combined byte count observed across both streams.
    pub output_bytes: u64,
    /// Output exceeded the capture budget.
    pub output_truncated: bool,
    /// Redacted, bounded stdout tail.
    pub stdout: String,
    /// Redacted, bounded stderr tail.
    pub stderr: String,
}

/// Runs a prepared command with a bounded wall clock and bounded capture.
pub(super) fn run_bounded(
    mut command: Command,
    timeout: Duration,
    redactor: &Redactor,
) -> BoundedOutput {
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) => {
            return BoundedOutput {
                exit_code: None,
                timed_out: false,
                spawn_error: Some(format!("failed to start a child process: {error}")),
                output_bytes: 0,
                output_truncated: false,
                stdout: String::new(),
                stderr: String::new(),
            };
        }
    };
    let total = Arc::new(AtomicU64::new(0));
    let stdout = child.stdout.take().map(|pipe| pump(pipe, total.clone()));
    let stderr = child.stderr.take().map(|pipe| pump(pipe, total.clone()));
    let deadline = Instant::now() + timeout;
    let mut timed_out = false;
    let mut exit_code = None;
    let mut spawn_error = None;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                exit_code = status.code();
                break;
            }
            Ok(None) if Instant::now() >= deadline => {
                shell::kill_tree(&mut child);
                timed_out = true;
                break;
            }
            Ok(None) => std::thread::sleep(POLL),
            Err(error) => {
                shell::kill_tree(&mut child);
                spawn_error = Some(format!("failed to wait for a child process: {error}"));
                break;
            }
        }
    }
    // A killed executor's descendants can keep a pipe open; the cleanup
    // budget bounds how long an attempt may wait for them to disappear.
    let stdout_redacted = join(stdout, redactor, shell::PROCESS_CLEANUP_BUDGET);
    let stderr_redacted = join(stderr, redactor, shell::PROCESS_CLEANUP_BUDGET);
    let observed = total.load(Ordering::Relaxed);
    BoundedOutput {
        exit_code,
        timed_out,
        spawn_error,
        output_bytes: observed,
        output_truncated: observed > OUTPUT_LIMIT_BYTES as u64,
        stdout: stdout_redacted,
        stderr: stderr_redacted,
    }
}

/// Drains one pipe, keeping only a bounded tail and the total byte count.
fn pump(
    mut pipe: impl Read + Send + 'static,
    total: Arc<AtomicU64>,
) -> std::thread::JoinHandle<String> {
    std::thread::spawn(move || {
        let mut tail: VecDeque<u8> = VecDeque::new();
        let mut chunk = [0_u8; CHUNK_BYTES];
        loop {
            match pipe.read(&mut chunk) {
                Ok(0) | Err(_) => break,
                Ok(read) => {
                    total.fetch_add(read as u64, Ordering::Relaxed);
                    for byte in &chunk[..read] {
                        if tail.len() == OUTPUT_LIMIT_BYTES {
                            tail.pop_front();
                        }
                        tail.push_back(*byte);
                    }
                }
            }
        }
        String::from_utf8_lossy(&tail.into_iter().collect::<Vec<u8>>()).into_owned()
    })
}

/// Joins a reader thread within `budget`, detaching it if it overruns.
fn join(
    handle: Option<std::thread::JoinHandle<String>>,
    redactor: &Redactor,
    budget: Duration,
) -> String {
    let Some(handle) = handle else {
        return String::new();
    };
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = sender.send(handle.join().ok());
    });
    match receiver.recv_timeout(budget) {
        Ok(Some(tail)) => redactor.text(&tail),
        _ => String::new(),
    }
}

/// Reads and validates the executor's declared report.
fn read_report(
    path: &Path,
    required: bool,
    redactor: &Redactor,
) -> (Option<ExecutorReport>, Option<String>) {
    match std::fs::metadata(path) {
        Ok(metadata) if metadata.len() > MAX_RESULT_BYTES as u64 => {
            return (
                None,
                Some(format!("executor result exceeds {MAX_RESULT_BYTES} bytes")),
            );
        }
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return (
                None,
                Some(redactor.text(&format!("failed to inspect executor result: {error}"))),
            );
        }
    }
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return if required {
                (
                    None,
                    Some("executor wrote no result file but the spec requires one".to_owned()),
                )
            } else {
                (None, None)
            };
        }
        Err(error) => {
            return (
                None,
                Some(redactor.text(&format!("failed to read executor result: {error}"))),
            );
        }
    };
    match serde_json::from_str::<ExecutorReport>(&text) {
        Ok(report) => {
            if report.status == super::receipt::ExecutorReportStatus::Blocked
                && report.blocked.is_none()
            {
                return (
                    None,
                    Some("executor reported blocked without a blocked claim".to_owned()),
                );
            }
            (Some(report), None)
        }
        Err(error) => (
            None,
            Some(redactor.text(&format!("malformed executor result: {error}"))),
        ),
    }
}
