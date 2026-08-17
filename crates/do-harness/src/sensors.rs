//! Computational sensor runner for `do-harness verify`.

use std::path::Path;
use std::process::Command;
use std::time::Instant;

use anyhow::{Result, anyhow};

use crate::config::{Config, SensorSpec};
use crate::report::{SensorResult, VerifyReport};

/// Options controlling a verify run.
pub struct VerifyOpts {
    /// Halt at the first failing sensor.
    pub fail_fast: bool,
    /// Restrict execution to these sensor names; empty = all.
    pub only: Vec<String>,
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

    let mut results: Vec<SensorResult> = Vec::new();
    for spec in sensors {
        if !opts.only.is_empty() && !opts.only.contains(&spec.name) {
            continue;
        }
        let result = run_sensor(spec, root);
        let failed = !result.ok;
        results.push(result);
        if failed && opts.fail_fast {
            break;
        }
    }

    let failed: Vec<String> = results
        .iter()
        .filter(|r| !r.ok)
        .map(|r| r.name.clone())
        .collect();
    Ok(VerifyReport {
        ok: failed.is_empty(),
        root: root.display().to_string(),
        failed,
        sensors: results,
    })
}

/// Runs a single sensor command from `root`, capturing combined output.
fn run_sensor(spec: &SensorSpec, root: &Path) -> SensorResult {
    let start = Instant::now();
    let Some((program, rest)) = spec.argv.split_first() else {
        return SensorResult {
            name: spec.name.clone(),
            ok: false,
            exit_code: None,
            duration_ms: 0,
            output: format!("sensor '{}' has an empty argv", spec.name),
        };
    };
    let output = Command::new(program).args(rest).current_dir(root).output();
    let duration_ms = u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX);

    match output {
        Ok(output) => SensorResult {
            name: spec.name.clone(),
            ok: output.status.success(),
            exit_code: output.status.code(),
            duration_ms,
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
            output: format!("failed to spawn {program}: {err}"),
        },
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
            },
        )
        .expect_err("verify must fail");
        assert!(err.to_string().contains("nope"));
        assert!(err.to_string().contains("pass"));
    }

    /// With fail_fast, execution stops at the first failing sensor.
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
            },
        )
        .expect("verify");
        assert!(!report.ok);
        assert_eq!(report.sensors.len(), 1);
        assert_eq!(report.sensors[0].name, "fail");
        assert_eq!(report.failed, vec!["fail".to_owned()]);
    }
}
