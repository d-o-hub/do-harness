//! Bounded, redacted capture and process-tree lifecycle for local sensors.
use std::collections::VecDeque;
use std::io::Read;
use std::process::{Child, ExitStatus};
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU64, Ordering},
    mpsc,
};
use std::thread;
use std::time::{Duration, Instant};

use crate::config::{self, SensorSpec};
use crate::redaction::Redactor;
use crate::report::{ExecutionDetails, FailureReason};

const POLL: Duration = Duration::from_millis(10);
const CHUNK_BYTES: usize = 8192;

pub(super) struct CapturedProcess {
    pub status: Option<ExitStatus>,
    pub output: String,
    pub details: ExecutionDetails,
    pub warned: bool,
    pub findings: Option<u64>,
}

struct PipeOutput {
    bytes: Vec<u8>,
    read_error: bool,
    warned: bool,
    findings: Option<u64>,
}

fn inspect_marker(line: &[u8], warned: &mut bool, findings: &mut Option<u64>) {
    let text = String::from_utf8_lossy(line);
    let text = text.trim_start();
    *warned |= text.starts_with("SKIP:");
    if let Some(count) = text
        .strip_prefix("FINDINGS:")
        .and_then(|s| s.trim().parse().ok())
    {
        *findings = Some(count);
    }
}

fn append_tail(tail: &mut VecDeque<u8>, bytes: &[u8], limit: usize) {
    for byte in bytes {
        if tail.len() == limit {
            tail.pop_front();
        }
        tail.push_back(*byte);
    }
}

fn pump(
    mut pipe: impl Read + Send + 'static,
    redactor: Redactor,
    limit: usize,
    total: Arc<AtomicU64>,
    oversized: Arc<AtomicBool>,
) -> mpsc::Receiver<PipeOutput> {
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let mut stream = redactor.stream();
        let mut tail = VecDeque::with_capacity(limit);
        let mut chunk = [0; CHUNK_BYTES];
        let mut read_error = false;
        let mut line = Vec::new();
        let mut warned = false;
        let mut findings = None;
        loop {
            match pipe.read(&mut chunk) {
                Ok(0) => break,
                Ok(count) => {
                    let count_u64 = u64::try_from(count).unwrap_or(u64::MAX);
                    let observed = total
                        .fetch_add(count_u64, Ordering::SeqCst)
                        .saturating_add(count_u64);
                    if observed > u64::try_from(limit).unwrap_or(u64::MAX) {
                        oversized.store(true, Ordering::SeqCst);
                    }
                    for byte in &chunk[..count] {
                        if *byte == b'\n' {
                            inspect_marker(&line, &mut warned, &mut findings);
                            line.clear();
                        } else if line.len() < limit {
                            line.push(*byte);
                        }
                    }
                    append_tail(&mut tail, &stream.feed(&chunk[..count], false), limit);
                }
                Err(_) => {
                    read_error = true;
                    break;
                }
            }
        }
        append_tail(&mut tail, &stream.feed(&[], true), limit);
        inspect_marker(&line, &mut warned, &mut findings);
        let _ = sender.send(PipeOutput {
            bytes: tail.into(),
            read_error,
            warned,
            findings,
        });
    });
    receiver
}

/// Drain both streams while enforcing timeout, fail-fast cancellation and output
/// policy. Pumps never block the caller past the fixed cleanup budget.
pub(super) fn capture(
    mut child: Child,
    spec: &SensorSpec,
    cancel: &AtomicBool,
    start: Instant,
    redactor: &Redactor,
) -> CapturedProcess {
    let limit = spec
        .max_output_bytes
        .unwrap_or(config::DEFAULT_OUTPUT_BYTES)
        .clamp(config::MIN_OUTPUT_BYTES, config::MAX_OUTPUT_BYTES);
    let total = Arc::new(AtomicU64::new(0));
    let oversized = Arc::new(AtomicBool::new(false));
    let mut receivers = Vec::new();
    if let Some(pipe) = child.stdout.take() {
        receivers.push(pump(
            pipe,
            redactor.clone(),
            limit,
            Arc::clone(&total),
            Arc::clone(&oversized),
        ));
    }
    if let Some(pipe) = child.stderr.take() {
        receivers.push(pump(
            pipe,
            redactor.clone(),
            limit,
            Arc::clone(&total),
            Arc::clone(&oversized),
        ));
    }
    let timeout = match spec.timeout {
        Some(0) => None,
        secs => Some(Duration::from_secs(
            secs.unwrap_or(config::DEFAULT_SENSOR_TIMEOUT_SECS),
        )),
    };
    let mut status = None;
    let mut reason = None;
    let mut cleanup = None;
    let mut streams: Vec<Option<PipeOutput>> = receivers.iter().map(|_| None).collect();
    loop {
        for (receiver, slot) in receivers.iter().zip(&mut streams) {
            if slot.is_none() {
                *slot = receiver.try_recv().ok();
            }
        }
        if reason.is_none() {
            if cancel.load(Ordering::SeqCst) {
                reason = Some(FailureReason::Cancelled);
            } else if oversized.load(Ordering::SeqCst) {
                reason = Some(FailureReason::OutputLimit);
            } else if timeout.is_some_and(|budget| start.elapsed() >= budget) {
                reason = Some(FailureReason::Timeout);
            }
        }
        if status.is_none() && cleanup.is_none() {
            match child.try_wait() {
                Ok(found) => status = found,
                Err(_) => reason = Some(FailureReason::Wait),
            }
        }
        if (status.is_some() || reason.is_some()) && cleanup.is_none() {
            crate::shell::kill_tree(&mut child);
            cleanup = Some(Instant::now());
        }
        if streams.iter().all(Option::is_some) && cleanup.is_some() {
            break;
        }
        if cleanup.is_some_and(|since| since.elapsed() >= crate::shell::PROCESS_CLEANUP_BUDGET) {
            reason = Some(FailureReason::CleanupUnverified);
            break;
        }
        thread::sleep(POLL);
    }
    if streams.iter().any(Option::is_none) {
        reason = Some(FailureReason::CleanupUnverified);
    } else if oversized.load(Ordering::SeqCst) {
        reason = Some(FailureReason::OutputLimit);
    }
    if streams.iter().flatten().any(|s| s.read_error) {
        reason = Some(FailureReason::Wait);
    }
    let output_bytes = total.load(Ordering::SeqCst);
    let output_truncated = oversized.load(Ordering::SeqCst)
        || streams.iter().any(Option::is_none)
        || streams.iter().flatten().any(|s| s.read_error);
    let warned = streams.iter().flatten().any(|s| s.warned);
    let findings = streams.iter().rev().flatten().find_map(|s| s.findings);
    let output = streams
        .into_iter()
        .flatten()
        .map(|s| String::from_utf8_lossy(&s.bytes).into_owned())
        .collect::<Vec<_>>()
        .join("\n");
    CapturedProcess {
        warned,
        findings,
        status,
        output,
        details: ExecutionDetails {
            reason,
            output_bytes,
            output_truncated,
        },
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]
    use super::*;
    use std::process::{Command, Stdio};

    #[test]
    fn success_and_failure_capture_are_redacted_before_retention() {
        let root = tempfile::tempdir().unwrap();
        let spec = crate::config::rust_pack().remove(0);
        for code in [0, 1] {
            let mut command = Command::new("python3");
            command.args(["-c", &format!("import sys;print('synthetic-private');sys.stderr.write('synthetic-private');raise SystemExit({code})")])
                .stdout(Stdio::piped()).stderr(Stdio::piped()).current_dir(root.path());
            crate::shell::configure_process_group(&mut command);
            let result = capture(
                command.spawn().unwrap(),
                &spec,
                &AtomicBool::new(false),
                Instant::now(),
                &Redactor::synthetic(&["synthetic-private"]),
            );
            assert!(!result.output.contains("synthetic-private"));
            assert!(result.output.contains("[REDACTED]"));
            assert_eq!(result.status.unwrap().code(), Some(code));
        }
    }
}
