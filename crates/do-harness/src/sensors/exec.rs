//! Single-sensor process execution for `do-harness verify`.
//!
//! Every attempt goes through one spawn-and-poll path: the child is spawned
//! with piped stdio, pump threads drain both pipes, and a poll loop watches
//! for exit, timeout expiry, and fail-fast cancellation. A shared cancel flag
//! lets the parallel driver kill in-flight siblings; the flag is never set on
//! sequential non-fail-fast runs, so that path is unaffected.

use std::io::Read;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use crate::config::SensorSpec;
use crate::report::SensorResult;

/// Poll interval for child exit, timeout, and cancel checks.
const POLL_INTERVAL: Duration = Duration::from_millis(10);

/// Elapsed milliseconds since `start`, saturating at `u64::MAX`.
fn elapsed_ms(start: Instant) -> u64 {
    u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX)
}

/// Builds a [`SensorResult`] for `spec`, deriving `warned` from a passing run
/// whose output carries a `SKIP:` marker (a tool or runtime was unavailable).
pub(crate) fn sensor_result(
    spec: &SensorSpec,
    ok: bool,
    exit_code: Option<i32>,
    duration_ms: u64,
    output: String,
) -> SensorResult {
    let warned = ok
        && output
            .lines()
            .any(|line| line.trim_start().starts_with("SKIP:"));
    SensorResult {
        name: spec.name.clone(),
        ok,
        exit_code,
        duration_ms,
        allow_failure: spec.allow_failure,
        warned,
        output,
    }
}

/// Spawns the sensor command with piped stdio, reporting a spawn failure as
/// a failed [`SensorResult`] instead of an error.
fn spawn_sensor(
    spec: &SensorSpec,
    root: &Path,
    program: &str,
    rest: &[String],
    start: Instant,
) -> Result<Child, SensorResult> {
    Command::new(program)
        .args(rest)
        .current_dir(root)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|err| {
            sensor_result(
                spec,
                false,
                None,
                elapsed_ms(start),
                format!("failed to spawn {program}: {err}"),
            )
        })
}

/// Spawns a thread that drains one piped stream to EOF.
fn pump(mut pipe: impl Read + Send + 'static) -> thread::JoinHandle<Vec<u8>> {
    thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = pipe.read_to_end(&mut buf);
        buf
    })
}

/// Joins the pump threads and concatenates stdout then stderr.
fn drain_pipes(
    stdout: Option<thread::JoinHandle<Vec<u8>>>,
    stderr: Option<thread::JoinHandle<Vec<u8>>>,
) -> String {
    let stdout = stdout.and_then(|h| h.join().ok()).unwrap_or_default();
    let stderr = stderr.and_then(|h| h.join().ok()).unwrap_or_default();
    format!(
        "{}\n{}",
        String::from_utf8_lossy(&stdout),
        String::from_utf8_lossy(&stderr)
    )
}

/// Kills and reaps the child, returning a failed result with `message`.
fn kill_and_report(
    child: &mut Child,
    spec: &SensorSpec,
    start: Instant,
    message: String,
) -> SensorResult {
    let _ = child.kill();
    let _ = child.wait();
    sensor_result(spec, false, None, elapsed_ms(start), message)
}

/// Runs a single sensor attempt from `root`, enforcing the spec timeout and
/// the shared fail-fast cancel flag.
///
/// A set cancel flag kills the child and reports a non-pass `cancelled`
/// result; timeout expiry kills the child and reports a non-pass timeout.
pub(crate) fn run_sensor_attempt(
    spec: &SensorSpec,
    root: &Path,
    cancel: &AtomicBool,
) -> SensorResult {
    let start = Instant::now();
    let Some((program, rest)) = spec.argv.split_first() else {
        return sensor_result(
            spec,
            false,
            None,
            0,
            format!("sensor '{}' has an empty argv", spec.name),
        );
    };
    let timeout = spec.timeout.map(Duration::from_secs);

    let mut child = match spawn_sensor(spec, root, program, rest, start) {
        Ok(child) => child,
        Err(result) => return result,
    };
    let stdout = child.stdout.take().map(pump);
    let stderr = child.stderr.take().map(pump);

    loop {
        if cancel.load(Ordering::SeqCst) {
            return kill_and_report(
                &mut child,
                spec,
                start,
                format!(
                    "sensor '{}' cancelled: fail-fast stopped this run",
                    spec.name
                ),
            );
        }
        match child.try_wait() {
            Ok(Some(status)) => {
                let output = drain_pipes(stdout, stderr);
                return sensor_result(
                    spec,
                    status.success(),
                    status.code(),
                    elapsed_ms(start),
                    output,
                );
            }
            Ok(None) => {
                if timeout.is_some_and(|budget| start.elapsed() >= budget) {
                    let secs = timeout.map_or(0, |budget| budget.as_secs());
                    return kill_and_report(
                        &mut child,
                        spec,
                        start,
                        format!("sensor '{}' timed out after {secs}s", spec.name),
                    );
                }
                thread::sleep(POLL_INTERVAL);
            }
            Err(err) => {
                return kill_and_report(
                    &mut child,
                    spec,
                    start,
                    format!("error waiting for child {program}: {err}"),
                );
            }
        }
    }
}

/// Runs a single sensor command from `root`, retrying on transient failures.
///
/// A set cancel flag stops further attempts after the attempt in flight.
pub(crate) fn run_sensor(spec: &SensorSpec, root: &Path, cancel: &AtomicBool) -> SensorResult {
    let max_retries = spec
        .retry
        .unwrap_or(if spec.transient_exit_codes.is_empty() {
            0
        } else {
            3
        });

    let mut attempts = 0;
    loop {
        let result = run_sensor_attempt(spec, root, cancel);
        if result.ok {
            return result;
        }

        if attempts >= max_retries || (attempts > 0 && cancel.load(Ordering::SeqCst)) {
            return result;
        }

        if !spec.transient_exit_codes.is_empty() {
            let is_transient = result
                .exit_code
                .is_some_and(|code| spec.transient_exit_codes.contains(&code));
            if !is_transient {
                return result;
            }
        }

        attempts += 1;
        thread::sleep(Duration::from_millis(50));
    }
}
