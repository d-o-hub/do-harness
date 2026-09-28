//! Report types and printers for `do-harness verify` and `do-harness list`.

use clap::ValueEnum;
use serde::Serialize;

/// Output format for reports and listings.
#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Format {
    /// Human-readable text output.
    #[default]
    Text,
    /// Machine-readable JSON output.
    Json,
}

/// How a sensor produced its verdict.
///
/// Reuse is never reported as an observed pass: a reused result carries no
/// exit code or duration and serializes as `"reused"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Execution {
    /// The sensor process executed in this run.
    #[default]
    Ran,
    /// A recorded passing beat was reused because the sensor's declared
    /// inputs are unchanged.
    Reused,
    /// The sensor did not execute: blocked, quarantined, or cancelled.
    NotRun,
}

/// Sensor verdict with timing and exit information.
#[derive(Debug, Clone, Serialize)]
pub struct SensorResult {
    /// Sensor name as configured.
    pub name: String,
    /// Whether the sensor exited successfully.
    pub ok: bool,
    /// Process exit code, or None when the process could not be spawned.
    pub exit_code: Option<i32>,
    /// Wall-clock duration of the run in milliseconds.
    pub duration_ms: u64,
    /// Effective severity as configured (`error` | `warn`).
    pub severity: crate::config::SensorSeverity,
    /// Whether this sensor failure was allowed/advisory (soft failure):
    /// warn severity, a below-baseline findings count, or a quarantine.
    #[serde(default)]
    pub allow_failure: bool,
    /// Whether a passing sensor reported a `SKIP:` marker because a tool or
    /// runtime was unavailable, or findings within the blessed baseline;
    /// evidence records this as a non-pass.
    #[serde(default)]
    pub warned: bool,
    /// Findings count reported by a `FINDINGS: <n>` marker, when present.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub findings: Option<u64>,
    /// Blessed ratchet baseline for this sensor, when one exists.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub baseline: Option<u64>,
    /// How this verdict was produced (`ran` | `reused` | `not_run`).
    #[serde(default)]
    pub execution: Execution,
    /// Recorded passing beat reused for an unchanged-input skip, when
    /// `execution == "reused"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reused_beat_id: Option<i64>,
    /// Captured combined output; excluded from serialization.
    #[serde(skip)]
    pub output: String,
}

/// Aggregate verify report. Serializes to the stable JSON contract.
#[derive(Debug, Clone, Default, Serialize)]
pub struct VerifyReport {
    /// True when every sensor passed.
    pub ok: bool,
    /// Workspace root the sensors ran from.
    pub root: String,
    /// Names of failing sensors in run order.
    pub failed: Vec<String>,
    /// Per-sensor results in run order.
    pub sensors: Vec<SensorResult>,
    /// Development signal set selected via `--set`; absent for full runs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signal_set: Option<String>,
}

/// Lines of failing-sensor output shown on stderr.
const OUTPUT_TAIL_LINES: usize = 80;

/// Prints a verify report in the requested format.
///
/// Verdict lines go to stdout in text mode; in JSON mode they go to stderr so
/// stdout carries exactly one JSON object. Failing-sensor output tails always
/// go to stderr, prefixed with six spaces.
pub fn print_report(report: &VerifyReport, format: Format) {
    for sensor in &report.sensors {
        let verdict = match sensor.execution {
            Execution::Reused => "SKIP",
            Execution::Ran | Execution::NotRun => {
                match (sensor.ok, sensor.warned, sensor.allow_failure) {
                    (true, false, _) => "PASS",
                    (false, _, false) => "FAIL",
                    (true, true, _) | (false, _, true) => "WARN",
                }
            }
        };
        let detail = if sensor.execution == Execution::Reused {
            match sensor.reused_beat_id {
                Some(id) => format!(" (unchanged inputs; reused beat {id})"),
                None => " (unchanged inputs; reused recorded passing beat)".to_owned(),
            }
        } else {
            findings_suffix(sensor)
        };
        let line = format!("{verdict}  {}{detail}", sensor.name);
        if format == Format::Json {
            eprintln!("{line}");
        } else {
            println!("{line}");
        }
        if !sensor.ok && sensor.execution != Execution::Reused {
            let lines: Vec<&str> = sensor.output.lines().collect();
            let start = lines.len().saturating_sub(OUTPUT_TAIL_LINES);
            for line in &lines[start..] {
                eprintln!("      {line}");
            }
        }
    }
    if report.ok {
        // A reused run is green but produced no fresh observations, so the
        // unconditional pass footer is replaced by an explicit reuse count.
        let reused = report
            .sensors
            .iter()
            .filter(|sensor| sensor.execution == Execution::Reused)
            .count();
        let footer = if reused == 0 {
            "All sensors passed.".to_owned()
        } else {
            format!("All sensors passed ({reused} reused from unchanged inputs).")
        };
        if format == Format::Json {
            eprintln!("{footer}");
        } else {
            println!("{footer}");
        }
    } else {
        eprintln!("Failed sensors: {}", report.failed.join(", "));
    }
    if format == Format::Json {
        match serde_json::to_writer_pretty(std::io::stdout(), report) {
            Ok(()) => println!(),
            Err(err) => eprintln!("error: failed to serialize report: {err}"),
        }
    }
}

/// Human-readable findings/baseline suffix for a sensor verdict line.
fn findings_suffix(sensor: &SensorResult) -> String {
    let Some(findings) = sensor.findings else {
        return String::new();
    };
    match sensor.baseline {
        Some(baseline) => {
            let delta = i64::try_from(findings).unwrap_or(i64::MAX)
                - i64::try_from(baseline).unwrap_or(i64::MAX);
            format!(" (findings {findings}, baseline {baseline}, delta {delta:+})")
        }
        None => format!(" (findings {findings})"),
    }
}

/// Prints a list of sensor names in the requested format.
pub fn print_names(names: &[String], format: Format) {
    match format {
        Format::Text => {
            for name in names {
                println!("{name}");
            }
        }
        Format::Json => match serde_json::to_string(names) {
            Ok(json) => println!("{json}"),
            Err(err) => eprintln!("error: failed to serialize names: {err}"),
        },
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    /// The JSON contract exposes verdict fields but never the raw output.
    #[test]
    fn json_shape_is_stable() {
        let report = VerifyReport {
            ok: false,
            root: "/tmp/root".to_owned(),
            failed: vec!["check".to_owned()],
            sensors: vec![SensorResult {
                name: "check".to_owned(),
                ok: false,
                exit_code: Some(1),
                duration_ms: 42,
                severity: crate::config::SensorSeverity::Error,
                allow_failure: false,
                warned: false,
                findings: None,
                baseline: None,
                execution: Execution::Ran,
                reused_beat_id: None,
                output: "hidden".to_owned(),
            }],
            signal_set: None,
        };
        let json = serde_json::to_string(&report).expect("serialize");
        let value: serde_json::Value = serde_json::from_str(&json).expect("parse");
        assert!(value.get("ok").is_some());
        assert!(value.get("root").is_some());
        assert!(value.get("failed").is_some());
        assert!(value["sensors"][0].get("exit_code").is_some());
        assert!(value["sensors"][0].get("warned").is_some());
        assert_eq!(value["sensors"][0]["execution"], "ran");
        assert!(value["sensors"][0].get("output").is_none());
    }
}
