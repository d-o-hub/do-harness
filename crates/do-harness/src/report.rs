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
    /// Whether this sensor failure was allowed/advisory (soft failure).
    #[serde(default)]
    pub allow_failure: bool,
    /// Captured combined output; excluded from serialization.
    #[serde(skip)]
    pub output: String,
}

/// Aggregate verify report. Serializes to the stable JSON contract.
#[derive(Debug, Clone, Serialize)]
pub struct VerifyReport {
    /// True when every sensor passed.
    pub ok: bool,
    /// Workspace root the sensors ran from.
    pub root: String,
    /// Names of failing sensors in run order.
    pub failed: Vec<String>,
    /// Per-sensor results in run order.
    pub sensors: Vec<SensorResult>,
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
        let verdict = if sensor.ok {
            "PASS"
        } else if sensor.allow_failure {
            "WARN"
        } else {
            "FAIL"
        };
        if format == Format::Json {
            eprintln!("{verdict}  {}", sensor.name);
        } else {
            println!("{verdict}  {}", sensor.name);
        }
        if !sensor.ok {
            let lines: Vec<&str> = sensor.output.lines().collect();
            let start = lines.len().saturating_sub(OUTPUT_TAIL_LINES);
            for line in &lines[start..] {
                eprintln!("      {line}");
            }
        }
    }
    if report.ok {
        if format == Format::Json {
            eprintln!("All sensors passed.");
        } else {
            println!("All sensors passed.");
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
                allow_failure: false,
                output: "hidden".to_owned(),
            }],
        };
        let json = serde_json::to_string(&report).expect("serialize");
        let value: serde_json::Value = serde_json::from_str(&json).expect("parse");
        assert!(value.get("ok").is_some());
        assert!(value.get("root").is_some());
        assert!(value.get("failed").is_some());
        assert!(value["sensors"][0].get("exit_code").is_some());
        assert!(value["sensors"][0].get("output").is_none());
    }
}
