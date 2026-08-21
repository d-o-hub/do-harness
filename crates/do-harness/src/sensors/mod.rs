//! Computational sensor runner for `do-harness verify`.

use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Result, anyhow};

use crate::config::{Config, SensorSpec};
use crate::report::{SensorResult, VerifyReport};
use crate::telemetry::FAIL_FAST_STRIKES;

/// Options controlling a verify run.
pub struct VerifyOpts {
    /// Halt at the first failing sensor.
    pub fail_fast: bool,
    /// Restrict execution to these sensor names; empty = all.
    pub only: Vec<String>,
    /// Sensor names halted by the fail-fast policy (not executed).
    pub blocked: Vec<String>,
}

/// Runs the selected sensors from `root` and returns the aggregate report.
///
/// # Errors
///
/// Returns an error when a name in `only` does not match any configured sensor.
pub fn verify(cfg: &Config, root: &Path, opts: &VerifyOpts) -> Result<VerifyReport> {
    let sensors = cfg.effective_sensors();
    let mut unknown: Vec<&str> = Vec::new();
    for name in &opts.only {
        if !sensors.iter().any(|s| &s.name == name) {
            unknown.push(name);
        }
    }
    if !unknown.is_empty() {
        let available = cfg.sensor_names().join(", ");
        return Err(anyhow!(
            "unknown sensor(s): {} (available: {available})",
            unknown.join(", ")
        ));
    }

    if sensors.is_empty() {
        return Ok(VerifyReport {
            ok: true,
            root: root.display().to_string(),
            failed: vec![],
            sensors: vec![],
        });
    }

    let mut results: Vec<SensorResult> = Vec::new();
    for spec in sensors {
        if !opts.only.is_empty() && !opts.only.contains(&spec.name) {
            continue;
        }
        let result = if opts.blocked.contains(&spec.name) {
            SensorResult {
                name: spec.name.clone(),
                ok: false,
                exit_code: None,
                duration_ms: 0,
                allow_failure: spec.allow_failure,
                output: format!(
                    "halted: sensor '{}' has failed {} consecutive times; resolve the underlying issue before re-running",
                    spec.name, FAIL_FAST_STRIKES
                ),
            }
        } else {
            run_sensor(spec, root)
        };
        let hard_failed = !result.ok && !result.allow_failure;
        results.push(result);
        if hard_failed && opts.fail_fast {
            break;
        }
    }

    let failed: Vec<String> = results
        .iter()
        .filter(|r| !r.ok && !r.allow_failure)
        .map(|r| r.name.clone())
        .collect();
    Ok(VerifyReport {
        ok: failed.is_empty(),
        root: root.display().to_string(),
        failed,
        sensors: results,
    })
}

/// Runs a single sensor attempt from `root`, enforcing timeouts if configured.
fn run_sensor_attempt(spec: &SensorSpec, root: &Path) -> SensorResult {
    let start = Instant::now();
    let Some((program, rest)) = spec.argv.split_first() else {
        return SensorResult {
            name: spec.name.clone(),
            ok: false,
            exit_code: None,
            duration_ms: 0,
            allow_failure: spec.allow_failure,
            output: format!("sensor '{}' has an empty argv", spec.name),
        };
    };

    let timeout_duration = spec.timeout.map(Duration::from_secs);

    if let Some(timeout) = timeout_duration {
        run_sensor_with_timeout(spec, root, program, rest, start, timeout)
    } else {
        let output = Command::new(program).args(rest).current_dir(root).output();
        let duration_ms = u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX);

        match output {
            Ok(output) => SensorResult {
                name: spec.name.clone(),
                ok: output.status.success(),
                exit_code: output.status.code(),
                duration_ms,
                allow_failure: spec.allow_failure,
                output: format!(
                    "{}\n{}",
                    String::from_utf8_lossy(&output.stdout),
                    String::from_utf8_lossy(&output.stderr)
                ),
            },
            Err(err) => SensorResult {
                name: spec.name.clone(),
                ok: false,
                exit_code: None,
                duration_ms,
                allow_failure: spec.allow_failure,
                output: format!("failed to spawn {program}: {err}"),
            },
        }
    }
}

/// Runs a single sensor command with timeout monitoring.
fn run_sensor_with_timeout(
    spec: &SensorSpec,
    root: &Path,
    program: &str,
    rest: &[String],
    start: Instant,
    timeout: Duration,
) -> SensorResult {
    let mut child = match Command::new(program)
        .args(rest)
        .current_dir(root)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(child) => child,
        Err(err) => {
            let duration_ms = u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX);
            return SensorResult {
                name: spec.name.clone(),
                ok: false,
                exit_code: None,
                duration_ms,
                allow_failure: spec.allow_failure,
                output: format!("failed to spawn {program}: {err}"),
            };
        }
    };

    let stdout_handle = child.stdout.take().map(|mut out| {
        thread::spawn(move || {
            let mut buf = Vec::new();
            let _ = out.read_to_end(&mut buf);
            buf
        })
    });

    let stderr_handle = child.stderr.take().map(|mut err| {
        thread::spawn(move || {
            let mut buf = Vec::new();
            let _ = err.read_to_end(&mut buf);
            buf
        })
    });

    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let duration_ms = u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX);
                let stdout = stdout_handle
                    .and_then(|h| h.join().ok())
                    .unwrap_or_default();
                let stderr = stderr_handle
                    .and_then(|h| h.join().ok())
                    .unwrap_or_default();
                return SensorResult {
                    name: spec.name.clone(),
                    ok: status.success(),
                    exit_code: status.code(),
                    duration_ms,
                    allow_failure: spec.allow_failure,
                    output: format!(
                        "{}\n{}",
                        String::from_utf8_lossy(&stdout),
                        String::from_utf8_lossy(&stderr)
                    ),
                };
            }
            Ok(None) => {
                if start.elapsed() >= timeout {
                    let _ = child.kill();
                    let _ = child.wait();
                    let duration_ms =
                        u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX);
                    return SensorResult {
                        name: spec.name.clone(),
                        ok: false,
                        exit_code: None,
                        duration_ms,
                        allow_failure: spec.allow_failure,
                        output: format!(
                            "sensor '{}' timed out after {}s",
                            spec.name,
                            timeout.as_secs()
                        ),
                    };
                }
                thread::sleep(Duration::from_millis(10));
            }
            Err(err) => {
                let _ = child.kill();
                let _ = child.wait();
                let duration_ms = u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX);
                return SensorResult {
                    name: spec.name.clone(),
                    ok: false,
                    exit_code: None,
                    duration_ms,
                    allow_failure: spec.allow_failure,
                    output: format!("error waiting for child {program}: {err}"),
                };
            }
        }
    }
}

/// Runs a single sensor command from `root`, retrying on transient failures.
fn run_sensor(spec: &SensorSpec, root: &Path) -> SensorResult {
    let max_retries = spec
        .retry
        .unwrap_or(if spec.transient_exit_codes.is_empty() {
            0
        } else {
            3
        });

    let mut attempts = 0;
    loop {
        let result = run_sensor_attempt(spec, root);
        if result.ok {
            return result;
        }

        if attempts >= max_retries {
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

#[cfg(test)]
mod tests;
