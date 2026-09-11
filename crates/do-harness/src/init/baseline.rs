//! Baseline execution for `do-harness init`.
//!
//! After generating (or adopting) the development contract, init runs it
//! once and reports the outcome. A green baseline is the only proof that the
//! generated signals actually run; a red baseline is surfaced and makes init
//! exit non-zero rather than claiming a verified workspace. Zero sensors is
//! reported as vacuous, never as meaningful evidence.

use std::path::Path;

use serde::Serialize;

use crate::config::Config;

/// Lines of a failing sensor's output kept for the baseline report.
const FAILURE_TAIL_LINES: usize = 20;

/// Baseline verification outcome.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum BaselineState {
    /// Every configured sensor passed.
    Green,
    /// At least one required sensor failed.
    Red,
    /// The config has no sensors; the pass carries no evidence.
    Vacuous,
}

/// One failing sensor's bounded diagnostic tail.
#[derive(Debug, Clone, Serialize)]
pub struct Failure {
    /// Sensor name.
    pub name: String,
    /// Last lines of captured output.
    pub detail: String,
}

/// Result of running the generated contract once.
#[derive(Debug, Clone, Serialize)]
pub struct Baseline {
    /// Aggregate state.
    pub state: BaselineState,
    /// Number of sensors that passed.
    pub passed: usize,
    /// Names of failing required sensors, in run order.
    pub failed: Vec<String>,
    /// Bounded output tails for the failing sensors.
    pub failures: Vec<Failure>,
}

/// Runs `cfg`'s effective sensors from `root` and summarizes the outcome.
///
/// # Errors
///
/// Returns an error when sensor selection is invalid (unknown `--only`
/// names cannot occur here, but config-level selection errors propagate).
pub fn run(cfg: &Config, root: &Path) -> anyhow::Result<Baseline> {
    if cfg.effective_sensors().is_empty() {
        return Ok(Baseline {
            state: BaselineState::Vacuous,
            passed: 0,
            failed: Vec::new(),
            failures: Vec::new(),
        });
    }
    let report = crate::sensors::verify(cfg, root, &crate::sensors::VerifyOpts::default())?;
    let failed: Vec<String> = report.failed.clone();
    let failures = report
        .sensors
        .iter()
        .filter(|sensor| !sensor.ok && !sensor.allow_failure)
        .map(|sensor| Failure {
            name: sensor.name.clone(),
            detail: tail(&sensor.output, FAILURE_TAIL_LINES),
        })
        .collect();
    let passed = report.sensors.iter().filter(|sensor| sensor.ok).count();
    Ok(Baseline {
        state: if report.ok {
            BaselineState::Green
        } else {
            BaselineState::Red
        },
        passed,
        failed,
        failures,
    })
}

/// Returns the last `lines` lines of `text`, trimmed.
fn tail(text: &str, lines: usize) -> String {
    let all: Vec<&str> = text.lines().collect();
    let start = all.len().saturating_sub(lines);
    all[start..].join("\n").trim().to_owned()
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    /// Builds a config with the given sensors under the generic pack so an
    /// empty list means genuinely zero effective sensors.
    fn config(sensors: Vec<crate::config::SensorSpec>) -> Config {
        Config {
            language: Some("generic".to_owned()),
            hooks: crate::config::HooksConfig::default(),
            signal_sets: std::collections::BTreeMap::new(),
            sensors,
        }
    }

    /// Builds a sensor spec running `argv`.
    fn spec(name: &str, argv: &[&str]) -> crate::config::SensorSpec {
        crate::config::SensorSpec {
            name: name.to_owned(),
            argv: argv.iter().map(ToString::to_string).collect(),
            retry: None,
            timeout: None,
            allow_failure: false,
            transient_exit_codes: Vec::new(),
            when_changed: Vec::new(),
        }
    }

    /// Zero sensors report vacuous, never green.
    #[test]
    fn zero_sensors_is_vacuous() {
        let dir = tempfile::tempdir().unwrap();
        let baseline = run(&config(vec![]), dir.path()).unwrap();
        assert_eq!(baseline.state, BaselineState::Vacuous);
    }

    /// Passing sensors report green.
    #[test]
    fn passing_sensors_are_green() {
        let dir = tempfile::tempdir().unwrap();
        let baseline = run(&config(vec![spec("ok", &["true"])]), dir.path()).unwrap();
        assert_eq!(baseline.state, BaselineState::Green);
        assert_eq!(baseline.passed, 1);
        assert!(baseline.failed.is_empty());
    }

    /// Failing sensors report red with a bounded diagnostic tail.
    #[test]
    fn failing_sensors_are_red_with_detail() {
        let dir = tempfile::tempdir().unwrap();
        let baseline = run(
            &config(vec![spec("bad", &["sh", "-c", "echo boom; exit 1"])]),
            dir.path(),
        )
        .unwrap();
        assert_eq!(baseline.state, BaselineState::Red);
        assert_eq!(baseline.failed, vec!["bad".to_owned()]);
        assert!(baseline.failures[0].detail.contains("boom"));
    }

    /// `allow_failure` failures do not make the baseline red.
    #[test]
    fn allow_failure_softens_baseline() {
        let dir = tempfile::tempdir().unwrap();
        let mut sensor = spec("advisory", &["false"]);
        sensor.allow_failure = true;
        let baseline = run(&config(vec![sensor]), dir.path()).unwrap();
        assert_eq!(baseline.state, BaselineState::Green);
    }
}
