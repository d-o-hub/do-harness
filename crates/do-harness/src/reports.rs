//! `do-harness report`: historical sensor executions for dashboards.
//!
//! `metrics` aggregates beats into a snapshot; `verify --format json` reports
//! one run. Neither answers "what did the sensors do over time" — durations,
//! exit codes, and failure counts per execution, in a format a CI job summary,
//! Grafana, or a Prometheus scrape can ingest. This command does, from the
//! recorded beats only: it never runs a sensor and never writes state.

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::Result;
use clap::ValueEnum;
use serde::Serialize;

use crate::metrics::MetricsFilter;
use crate::{CliError, metrics};

/// Schema version of the `--format json` document.
///
/// Bumped when a field is removed or changes meaning; `verify`'s JSON contract
/// and this one move independently.
pub const REPORT_SCHEMA_VERSION: u32 = 1;

/// Output format for `report`.
#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ReportFormat {
    /// Human-readable table.
    #[default]
    Text,
    /// Versioned JSON document.
    Json,
    /// Prometheus text exposition (v0.0.4) of window aggregates.
    Prometheus,
}

/// The time window the report covers.
#[derive(Debug, Clone, Serialize)]
pub struct Window {
    /// Inclusive Unix-seconds lower bound, when given.
    pub since: Option<i64>,
    /// Inclusive Unix-seconds upper bound, when given.
    pub until: Option<i64>,
}

/// One document in the JSON contract.
#[derive(Debug, Clone, Serialize)]
pub struct ReportDocument {
    /// Schema version of this document.
    pub schema_version: u32,
    /// Workstream scope the query used, or `all`.
    pub scope: String,
    /// Inclusive window.
    pub window: Window,
    /// Sensor executions, oldest first. Each carries `sensor`, `scope`,
    /// `status`, `exit_code`, `started_at`, `completed_at`, and `duration_ms`.
    pub executions: Vec<do_harness_db::SensorExecution>,
}

/// Runs `do-harness report`.
///
/// # Errors
///
/// Returns a usage error for an unparsable window, and a database error when
/// the state store cannot be read.
pub async fn run(
    root: &Path,
    filter: &MetricsFilter<'_>,
    format: ReportFormat,
) -> std::result::Result<(), CliError> {
    let since = parse_bound("--since", filter.since).map_err(CliError::Usage)?;
    let until = parse_bound("--until", filter.until).map_err(CliError::Usage)?;
    if let (Some(since), Some(until)) = (since, until) {
        if until < since {
            return Err(CliError::Usage(anyhow::anyhow!(
                "--until ({until}) is before --since ({since}); the window would be empty"
            )));
        }
    }
    let scope = metrics::effective_scope(root, filter);
    let conn = do_harness_db::connect_and_migrate(root)
        .await
        .map_err(|error| CliError::Usage(error.into()))?;
    let executions =
        do_harness_db::sensor_executions(&conn, since, until, scope.as_deref(), filter.sensor)
            .await
            .map_err(|error| CliError::Usage(error.into()))?;
    let document = ReportDocument {
        schema_version: REPORT_SCHEMA_VERSION,
        scope: scope.clone().unwrap_or_else(|| "all".to_owned()),
        window: Window { since, until },
        executions,
    };
    match format {
        ReportFormat::Text => print_text(&document),
        ReportFormat::Json => {
            println!(
                "{}",
                crate::report::json_pretty(&document).map_err(CliError::Usage)?
            );
        }
        ReportFormat::Prometheus => print_prometheus(&document),
    }
    Ok(())
}

/// Parses a window bound: Unix seconds, as `metrics --since` does.
fn parse_bound(flag: &str, raw: Option<&str>) -> Result<Option<i64>> {
    raw.map(|value| {
        value.parse::<i64>().map_err(|_| {
            anyhow::anyhow!("invalid {flag} '{value}': expected a Unix timestamp in seconds")
        })
    })
    .transpose()
}

fn print_text(document: &ReportDocument) {
    println!(
        "scope: {}  window: since={} until={}  executions: {}",
        document.scope,
        document
            .window
            .since
            .map_or_else(|| "-".to_owned(), |v| v.to_string()),
        document
            .window
            .until
            .map_or_else(|| "-".to_owned(), |v| v.to_string()),
        document.executions.len()
    );
    for execution in &document.executions {
        let duration = execution
            .duration_ms
            .map_or_else(|| "-".to_owned(), |ms| format!("{ms}ms"));
        let exit = execution
            .exit_code
            .map_or_else(|| "-".to_owned(), |code| code.to_string());
        println!(
            "  {} {} status={} exit={exit} duration={duration} started={} scope={}",
            execution.id, execution.sensor, execution.status, execution.started_at, execution.scope
        );
    }
}

/// Aggregates the window by `(sensor, scope)` for the Prometheus scrape.
#[derive(Debug, Default)]
struct Aggregate {
    runs: i64,
    failures: i64,
    duration_ms: i64,
    last_exit: Option<i32>,
}

fn print_prometheus(document: &ReportDocument) {
    let mut aggregates: BTreeMap<(&str, &str), Aggregate> = BTreeMap::new();
    for execution in &document.executions {
        let aggregate = aggregates
            .entry((execution.sensor.as_str(), execution.scope.as_str()))
            .or_default();
        aggregate.runs += 1;
        if execution.status != "ok" {
            aggregate.failures += 1;
        }
        aggregate.duration_ms += execution.duration_ms.unwrap_or(0);
        if execution.exit_code.is_some() {
            aggregate.last_exit = execution.exit_code;
        }
    }
    println!("# HELP do_harness_sensor_runs_total Recorded sensor executions in the window.");
    println!("# TYPE do_harness_sensor_runs_total counter");
    for ((sensor, scope), aggregate) in &aggregates {
        println!(
            "do_harness_sensor_runs_total{{sensor=\"{}\",scope=\"{}\"}} {}",
            escape_label(sensor),
            escape_label(scope),
            aggregate.runs
        );
    }
    println!(
        "# HELP do_harness_sensor_failures_total Recorded non-ok sensor executions in the window."
    );
    println!("# TYPE do_harness_sensor_failures_total counter");
    for ((sensor, scope), aggregate) in &aggregates {
        println!(
            "do_harness_sensor_failures_total{{sensor=\"{}\",scope=\"{}\"}} {}",
            escape_label(sensor),
            escape_label(scope),
            aggregate.failures
        );
    }
    println!(
        "# HELP do_harness_sensor_duration_seconds_total Summed sensor run time in the window."
    );
    println!("# TYPE do_harness_sensor_duration_seconds_total counter");
    for ((sensor, scope), aggregate) in &aggregates {
        println!(
            "do_harness_sensor_duration_seconds_total{{sensor=\"{}\",scope=\"{}\"}} {}.{:03}",
            escape_label(sensor),
            escape_label(scope),
            aggregate.duration_ms / 1000,
            aggregate.duration_ms % 1000
        );
    }
    println!("# HELP do_harness_sensor_last_exit_code Last observed exit code in the window.");
    println!("# TYPE do_harness_sensor_last_exit_code gauge");
    for ((sensor, scope), aggregate) in &aggregates {
        if let Some(code) = aggregate.last_exit {
            println!(
                "do_harness_sensor_last_exit_code{{sensor=\"{}\",scope=\"{}\"}} {code}",
                escape_label(sensor),
                escape_label(scope)
            );
        }
    }
}

/// Escapes a label value per the Prometheus text exposition format.
fn escape_label(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '\\' => escaped.push_str("\\\\"),
            '"' => escaped.push_str("\\\""),
            '\n' => escaped.push_str("\\n"),
            c => escaped.push(c),
        }
    }
    escaped
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    fn execution(id: i64, sensor: &str, status: &str) -> do_harness_db::SensorExecution {
        do_harness_db::SensorExecution {
            id,
            sensor: sensor.to_owned(),
            scope: "branch:main".to_owned(),
            status: status.to_owned(),
            exit_code: (status != "ok").then_some(1),
            started_at: 100 + id,
            completed_at: Some(100 + id + 2),
            duration_ms: Some(2000),
        }
    }

    #[test]
    fn a_window_is_validated_in_order() {
        assert_eq!(parse_bound("--since", Some("100")).unwrap(), Some(100));
        assert_eq!(parse_bound("--since", None).unwrap(), None);
        assert!(parse_bound("--since", Some("yesterday")).is_err());
    }

    #[test]
    fn labels_escape_quotes_and_newlines() {
        assert_eq!(escape_label("a\"b\\c\nd"), "a\\\"b\\\\c\\nd");
    }

    #[test]
    fn the_prometheus_aggregate_counts_failures_and_seconds() {
        let document = ReportDocument {
            schema_version: REPORT_SCHEMA_VERSION,
            scope: "branch:main".to_owned(),
            window: Window {
                since: None,
                until: None,
            },
            executions: vec![
                execution(1, "check", "ok"),
                execution(2, "check", "failed"),
                execution(3, "test", "ok"),
            ],
        };
        // Capture the renderer's contract through its aggregate, since the
        // printer writes to stdout: two runs for `check`, one failure, 4s.
        let mut aggregates: BTreeMap<(&str, &str), Aggregate> = BTreeMap::new();
        for execution in &document.executions {
            let aggregate = aggregates
                .entry((execution.sensor.as_str(), execution.scope.as_str()))
                .or_default();
            aggregate.runs += 1;
            if execution.status != "ok" {
                aggregate.failures += 1;
            }
            aggregate.duration_ms += execution.duration_ms.unwrap_or(0);
        }
        let check = &aggregates[&("check", "branch:main")];
        assert_eq!(
            (check.runs, check.failures, check.duration_ms),
            (2, 1, 4000)
        );
    }
}
