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
                    let duration_ms =
                        u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX);
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
                    let duration_ms =
                        u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX);
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

/// Runs a single sensor command from `root`, retrying on transient failures.
fn run_sensor(spec: &SensorSpec, root: &Path) -> SensorResult {
    let max_retries = spec
        .retry
        .unwrap_or(if !spec.transient_exit_codes.is_empty() {
            3
        } else {
            0
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
                .map_or(false, |code| spec.transient_exit_codes.contains(&code));
            if !is_transient {
                return result;
            }
        }

        attempts += 1;
        thread::sleep(Duration::from_millis(50));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Config, HooksConfig};

    fn config_with(specs: &[(&str, &[&str])]) -> Config {
        Config {
            language: None,
            hooks: HooksConfig {
                pre_commit: vec![],
                pre_push: vec![],
            },
            sensors: specs
                .iter()
                .map(|(name, argv)| SensorSpec {
                    name: (*name).to_owned(),
                    argv: argv.iter().map(|a| (*a).to_owned()).collect(),
                    retry: None,
                    timeout: None,
                    allow_failure: false,
                    transient_exit_codes: vec![],
                })
                .collect(),
        }
    }

    /// A failing sensor fails the report and records its exit code.
    #[test]
    fn failing_sensor_fails_report() {
        let dir = tempfile::tempdir().expect("tempdir");
        let cfg = config_with(&[("pass", &["true"]), ("fail", &["false"])]);
        let report = verify(
            &cfg,
            dir.path(),
            &VerifyOpts {
                fail_fast: false,
                only: vec![],
                blocked: vec![],
            },
        )
        .expect("verify");
        assert!(!report.ok);
        assert_eq!(report.failed, vec!["fail".to_owned()]);
        let exit_codes: Vec<Option<i32>> = report.sensors.iter().map(|s| s.exit_code).collect();
        assert_eq!(exit_codes, vec![Some(0), Some(1)]);
        let oks: Vec<bool> = report.sensors.iter().map(|s| s.ok).collect();
        assert_eq!(oks, vec![true, false]);
    }

    /// The `only` filter restricts execution to the named sensor.
    #[test]
    fn only_filter_runs_subset() {
        let dir = tempfile::tempdir().expect("tempdir");
        let cfg = config_with(&[("pass", &["true"]), ("fail", &["false"])]);
        let report = verify(
            &cfg,
            dir.path(),
            &VerifyOpts {
                fail_fast: false,
                only: vec!["pass".to_owned()],
                blocked: vec![],
            },
        )
        .expect("verify");
        assert!(report.ok);
        assert_eq!(report.sensors.len(), 1);
        assert_eq!(report.sensors[0].name, "pass");
    }

    /// An unknown `only` name is rejected before anything runs.
    #[test]
    fn unknown_only_name_errors() {
        let dir = tempfile::tempdir().expect("tempdir");
        let cfg = config_with(&[("pass", &["true"])]);
        let err = verify(
            &cfg,
            dir.path(),
            &VerifyOpts {
                fail_fast: false,
                only: vec!["nope".to_owned()],
                blocked: vec![],
            },
        )
        .expect_err("verify must fail");
        assert!(err.to_string().contains("nope"));
        assert!(err.to_string().contains("pass"));
    }

    /// With `fail_fast`, execution stops at the first failing sensor.
    #[test]
    fn fail_fast_stops_at_first_failure() {
        let dir = tempfile::tempdir().expect("tempdir");
        let cfg = config_with(&[("fail", &["false"]), ("true", &["true"])]);
        let report = verify(
            &cfg,
            dir.path(),
            &VerifyOpts {
                fail_fast: true,
                only: vec![],
                blocked: vec![],
            },
        )
        .expect("verify");
        assert!(!report.ok);
        assert_eq!(report.sensors.len(), 1);
        assert_eq!(report.sensors[0].name, "fail");
        assert_eq!(report.failed, vec!["fail".to_owned()]);
    }

    /// A generic pack with no sensors verifies trivially.
    #[test]
    fn verify_with_no_effective_sensors_succeeds() {
        let dir = tempfile::tempdir().expect("tempdir");
        let cfg = Config {
            language: Some("generic".to_owned()),
            hooks: HooksConfig {
                pre_commit: vec![],
                pre_push: vec![],
            },
            sensors: vec![],
        };
        let report = verify(
            &cfg,
            dir.path(),
            &VerifyOpts {
                fail_fast: false,
                only: vec![],
                blocked: vec![],
            },
        )
        .expect("verify");
        assert!(report.ok);
        assert!(report.sensors.is_empty());
        assert!(report.failed.is_empty());
    }

    /// A blocked sensor is not executed: no marker file appears.
    #[test]
    fn blocked_sensor_is_not_executed() {
        let dir = tempfile::tempdir().expect("tempdir");
        let cfg = config_with(&[("mark", &["sh", "-c", "touch marker"])]);
        let report = verify(
            &cfg,
            dir.path(),
            &VerifyOpts {
                fail_fast: false,
                only: vec![],
                blocked: vec!["mark".to_owned()],
            },
        )
        .expect("verify");
        assert!(!report.sensors[0].ok);
        assert_eq!(report.sensors[0].exit_code, None);
        assert_eq!(report.sensors[0].duration_ms, 0);
        assert!(report.sensors[0].output.contains("halted"));
        assert!(!dir.path().join("marker").exists());
    }

    /// A blocked sensor fails the report while unblocked sensors still run.
    #[test]
    fn blocked_sensor_counts_as_failed() {
        let dir = tempfile::tempdir().expect("tempdir");
        let cfg = config_with(&[("ok", &["true"]), ("halt", &["true"])]);
        let report = verify(
            &cfg,
            dir.path(),
            &VerifyOpts {
                fail_fast: false,
                only: vec![],
                blocked: vec!["halt".to_owned()],
            },
        )
        .expect("verify");
        assert!(!report.ok);
        assert_eq!(report.failed, vec!["halt".to_owned()]);
        assert!(report.sensors[0].ok);
        assert_eq!(report.sensors[0].name, "ok");
        assert!(!report.sensors[1].ok);
    }

    /// With `fail_fast`, a blocked sensor halts the run like any failure.
    #[test]
    fn fail_fast_stops_at_first_blocked_sensor() {
        let dir = tempfile::tempdir().expect("tempdir");
        let cfg = config_with(&[("halt", &["true"]), ("after", &["true"])]);
        let report = verify(
            &cfg,
            dir.path(),
            &VerifyOpts {
                fail_fast: true,
                only: vec![],
                blocked: vec!["halt".to_owned()],
            },
        )
        .expect("verify");
        assert!(!report.ok);
        assert_eq!(report.sensors.len(), 1);
        assert_eq!(report.sensors[0].name, "halt");
        assert!(!report.sensors[0].ok);
        assert_eq!(report.failed, vec!["halt".to_owned()]);
    }

    /// A sensor configured to retry retries upon failure until it succeeds.
    #[test]
    fn retries_failing_sensor_until_success() {
        let dir = tempfile::tempdir().expect("tempdir");
        let counter_file = dir.path().join("counter.txt");
        let script = format!(
            "count=$(cat '{}' 2>/dev/null || echo 0)\ncount=$((count + 1))\necho $count > '{}'\nif [ $count -lt 3 ]; then exit 1; fi",
            counter_file.display(),
            counter_file.display()
        );
        let cfg = Config {
            language: None,
            hooks: HooksConfig::default(),
            sensors: vec![SensorSpec {
                name: "flaky".to_owned(),
                argv: vec!["sh".to_owned(), "-c".to_owned(), script],
                retry: Some(3),
                timeout: None,
                allow_failure: false,
                transient_exit_codes: vec![],
            }],
        };

        let report = verify(
            &cfg,
            dir.path(),
            &VerifyOpts {
                fail_fast: false,
                only: vec![],
                blocked: vec![],
            },
        )
        .expect("verify");

        assert!(report.ok);
        assert!(report.failed.is_empty());
        assert!(report.sensors[0].ok);
        assert_eq!(
            std::fs::read_to_string(&counter_file).expect("read counter").trim(),
            "3"
        );
    }

    /// A sensor configured with a timeout budget is aborted when it hangs.
    #[test]
    fn times_out_hanging_sensor() {
        let dir = tempfile::tempdir().expect("tempdir");
        let cfg = Config {
            language: None,
            hooks: HooksConfig::default(),
            sensors: vec![SensorSpec {
                name: "hang".to_owned(),
                argv: vec!["sleep".to_owned(), "10".to_owned()],
                retry: None,
                timeout: Some(1),
                allow_failure: false,
                transient_exit_codes: vec![],
            }],
        };

        let start = Instant::now();
        let report = verify(
            &cfg,
            dir.path(),
            &VerifyOpts {
                fail_fast: false,
                only: vec![],
                blocked: vec![],
            },
        )
        .expect("verify");

        let duration = start.elapsed();
        assert!(duration < Duration::from_secs(5));
        assert!(!report.ok);
        assert_eq!(report.failed, vec!["hang".to_owned()]);
        assert!(!report.sensors[0].ok);
        assert!(report.sensors[0].output.contains("timed out after 1s"));
    }

    /// An `allow_failure` sensor never flips the gate to fail, but failure is recorded.
    #[test]
    fn allow_failure_sensor_does_not_fail_gate_but_surfaces_output() {
        let dir = tempfile::tempdir().expect("tempdir");
        let cfg = Config {
            language: None,
            hooks: HooksConfig::default(),
            sensors: vec![SensorSpec {
                name: "advisory".to_owned(),
                argv: vec!["sh".to_owned(), "-c".to_owned(), "echo 'something wrong'; exit 1".to_owned()],
                retry: None,
                timeout: None,
                allow_failure: true,
                transient_exit_codes: vec![],
            }],
        };

        let report = verify(
            &cfg,
            dir.path(),
            &VerifyOpts {
                fail_fast: false,
                only: vec![],
                blocked: vec![],
            },
        )
        .expect("verify");

        assert!(report.ok);
        assert!(report.failed.is_empty());
        assert!(!report.sensors[0].ok);
        assert!(report.sensors[0].allow_failure);
        assert!(report.sensors[0].output.contains("something wrong"));
    }

    /// Restricting retries to `transient_exit_codes` does not retry non-transient exit codes.
    #[test]
    fn transient_exit_codes_restricts_retries() {
        let dir = tempfile::tempdir().expect("tempdir");
        let counter_file = dir.path().join("transient_counter.txt");
        let script = format!(
            "count=$(cat '{}' 2>/dev/null || echo 0)\ncount=$((count + 1))\necho $count > '{}'\nexit 1",
            counter_file.display(),
            counter_file.display()
        );
        let cfg = Config {
            language: None,
            hooks: HooksConfig::default(),
            sensors: vec![SensorSpec {
                name: "transient_check".to_owned(),
                argv: vec!["sh".to_owned(), "-c".to_owned(), script],
                retry: Some(3),
                timeout: None,
                allow_failure: false,
                transient_exit_codes: vec![75],
            }],
        };

        let report = verify(
            &cfg,
            dir.path(),
            &VerifyOpts {
                fail_fast: false,
                only: vec![],
                blocked: vec![],
            },
        )
        .expect("verify");

        assert!(!report.ok);
        assert_eq!(report.failed, vec!["transient_check".to_owned()]);
        assert_eq!(
            std::fs::read_to_string(&counter_file).expect("read counter").trim(),
            "1"
        );
    }
}
