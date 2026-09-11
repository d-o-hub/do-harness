//! Computational sensor runner for `do-harness verify`.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Result, anyhow};

use crate::config::{Config, SensorSpec};
use crate::report::{Format, SensorResult, VerifyReport};
use crate::telemetry::FAIL_FAST_STRIKES;

/// Options controlling a verify run.
///
/// Each boolean maps 1:1 to an independent CLI flag (`--fail-fast`,
/// `--changed`, `--record`, `--strict`); they are not a state machine, so
/// the struct keeps one field per flag.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, Default)]
pub struct VerifyOpts {
    /// Halt at the first failing sensor.
    pub fail_fast: bool,
    /// Restrict execution to this development signal set; empty = all.
    pub set: Option<String>,
    /// Restrict execution to sensors applicable to the working-tree change.
    pub changed: bool,
    /// Restrict execution to these sensor names; empty = all.
    pub only: Vec<String>,
    /// Exclude these sensor names from execution.
    pub exclude: Vec<String>,
    /// Sensor names halted by the fail-fast policy (not executed).
    pub blocked: Vec<String>,
    /// Persist sensor beats to the state database.
    pub record: bool,
    /// Task id scoping persisted beats when `record` is set.
    pub task: Option<i64>,
    /// Evidence artifact path; relative paths resolve against the root.
    pub evidence: Option<PathBuf>,
    /// Fail the run when the evidence artifact is not strictly clean.
    pub strict: bool,
    /// Report output format.
    pub format: Format,
    /// Explicit config file override.
    pub config: Option<PathBuf>,
}

/// Runs the selected sensors from `root` and returns the aggregate report.
///
/// `--set` selects the candidate signal set; `--changed` keeps only the
/// applicable sensors; `--only` narrows within them and `--exclude` removes
/// from them.
///
/// # Errors
///
/// Returns an error when a name in `only` does not match any configured
/// sensor, when a name in `only` is outside the selected signal set, or when
/// the requested signal set is unknown or unconfigured.
pub fn verify(cfg: &Config, root: &Path, opts: &VerifyOpts) -> Result<VerifyReport> {
    let signal_set = opts.set.clone();
    let selection = resolve_selection(cfg, root, opts)?;

    if selection.specs.is_empty() {
        return Ok(VerifyReport {
            ok: true,
            root: root.display().to_string(),
            failed: vec![],
            sensors: vec![],
            signal_set,
        });
    }

    let mut results: Vec<SensorResult> = Vec::new();
    for spec in selection.specs {
        // Blocked synthesis: sensor is halted by fail-fast policy (ok=false, exit_code=None, duration_ms=0).
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
        signal_set,
    })
}

/// A fully resolved verify selection: the specs to execute plus the
/// change-aware reasoning behind them (for evidence and `explain` parity).
pub struct ResolvedSelection<'a> {
    /// Specs to execute, in run order.
    pub specs: Vec<&'a SensorSpec>,
    /// Sensors skipped by `--changed` applicability, with reasons.
    pub skipped: Vec<crate::applicability::Skipped>,
}

/// Resolves the ordered sensor specs a verify run executes.
///
/// `--set` defines the candidate list (in set order); `--changed` keeps only
/// the sensors applicable to the working-tree change (fail-closed: every
/// candidate when discovery fails); `--only` narrows within the applicable
/// list; `--exclude` removes from it. Without `--set` the effective sensor
/// list is used unchanged.
///
/// # Errors
///
/// Returns an error when a name in `only` is unknown, lies outside the
/// selected signal set, or when the requested signal set is unknown.
pub fn resolve_selection<'a>(
    cfg: &'a Config,
    root: &Path,
    opts: &VerifyOpts,
) -> Result<ResolvedSelection<'a>> {
    let sensors = cfg.effective_sensors();

    let effective_only: Vec<String> = opts
        .only
        .iter()
        .flat_map(|s| s.split(','))
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
        .collect();

    let effective_exclude: Vec<String> = opts
        .exclude
        .iter()
        .flat_map(|s| s.split(','))
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
        .collect();

    let mut unknown: Vec<&str> = Vec::new();
    for name in &effective_only {
        if !sensors.iter().any(|s| &s.name == name) {
            unknown.push(name);
        }
    }
    if !unknown.is_empty() {
        let available = cfg.sensor_names().join(", ");
        return Err(anyhow!(
            "unknown sensor(s): {} (available: {available}; run `do-harness list` to see configured sensors)",
            unknown.join(", ")
        ));
    }

    let ordered: Vec<&SensorSpec> = crate::signals::candidate_specs(cfg, opts.set.as_deref())?;
    if let Some(name) = &opts.set {
        let outside: Vec<&str> = effective_only
            .iter()
            .filter(|only| !ordered.iter().any(|spec| &spec.name == *only))
            .map(String::as_str)
            .collect();
        if !outside.is_empty() {
            let members: Vec<&str> = ordered.iter().map(|spec| spec.name.as_str()).collect();
            return Err(anyhow!(
                "sensor(s) {} not in signal set '{name}' (set sensors: {})",
                outside.join(", "),
                members.join(", ")
            ));
        }
    }

    let applicable: Vec<&SensorSpec> = if opts.changed {
        let changed = crate::changes::discover(root);
        let selection = crate::applicability::select(&ordered, &changed);
        let names = selection.selected_names();
        let skipped = selection.skipped;
        let specs: Vec<&SensorSpec> = ordered
            .into_iter()
            .filter(|spec| {
                names.contains(&spec.name)
                    && (effective_only.is_empty() || effective_only.contains(&spec.name))
                    && !effective_exclude.contains(&spec.name)
            })
            .collect();
        return Ok(ResolvedSelection { specs, skipped });
    } else {
        ordered
    };

    let specs: Vec<&SensorSpec> = applicable
        .into_iter()
        .filter(|spec| {
            (effective_only.is_empty() || effective_only.contains(&spec.name))
                && !effective_exclude.contains(&spec.name)
        })
        .collect();
    Ok(ResolvedSelection {
        specs,
        skipped: Vec::new(),
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
