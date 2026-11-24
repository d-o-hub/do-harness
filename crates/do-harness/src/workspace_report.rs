//! Stable aggregate output for multi-workspace verification.

use serde::Serialize;
use serde_json::Value;

use crate::report::{Execution, sensor_verdict_label};

#[derive(Debug, Serialize)]
pub(crate) struct WorkspaceReport {
    pub(crate) schema_version: u32,
    pub(crate) root: String,
    pub(crate) pattern: String,
    pub(crate) mode: &'static str,
    pub(crate) jobs: usize,
    pub(crate) ok: bool,
    pub(crate) exit_code: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) selection: Option<GlobalSelection>,
    pub(crate) workspaces: Vec<WorkspaceResult>,
}

#[derive(Debug, Serialize)]
pub(crate) struct GlobalSelection {
    pub(crate) mode: String,
    pub(crate) since: Option<String>,
    pub(crate) changed_files: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) discovery_error: Option<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct WorkspaceResult {
    pub(crate) root: String,
    pub(crate) status: &'static str,
    pub(crate) ok: bool,
    pub(crate) exit_code: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) report: Option<Value>,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub(crate) stderr: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) reason: Option<String>,
}

pub(crate) fn append_error(stderr: &mut String, message: &str) {
    if !stderr.is_empty() && !stderr.ends_with('\n') {
        stderr.push('\n');
    }
    stderr.push_str(message);
    stderr.push('\n');
}

pub(crate) fn aggregate_exit_code(results: &[WorkspaceResult]) -> i32 {
    if results.iter().any(|result| result.exit_code == 3) {
        3
    } else if results.iter().any(|result| result.exit_code == 1) {
        1
    } else if results.iter().any(|result| result.exit_code == 2) {
        2
    } else {
        0
    }
}

fn sensor_status(sensor: &Value, mode: &str) -> &'static str {
    if matches!(mode, "simulate" | "dry-run") {
        return "PLAN";
    }
    let execution = match sensor.get("execution").and_then(Value::as_str) {
        Some("ran") => Execution::Ran,
        Some("reused") => Execution::Reused,
        Some("not_run") => Execution::NotRun,
        Some("planned") => Execution::Planned,
        _ => return "FAIL",
    };
    sensor_verdict_label(
        execution,
        sensor.get("ok").and_then(Value::as_bool).unwrap_or(false),
        sensor
            .get("warned")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        sensor
            .get("allow_failure")
            .and_then(Value::as_bool)
            .unwrap_or(false),
    )
}

pub(crate) fn print_text(report: &WorkspaceReport, quiet: bool) {
    for workspace in &report.workspaces {
        if !quiet || workspace.status != "passed" {
            println!(
                "{}: {} (exit {})",
                workspace.root, workspace.status, workspace.exit_code
            );
        }
        if let Some(reason) = &workspace.reason {
            println!("  {reason}");
        }
        if let Some(sensors) = workspace
            .report
            .as_ref()
            .and_then(|value| value.get("sensors"))
            .and_then(Value::as_array)
        {
            for sensor in sensors {
                let name = sensor
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or("(unnamed sensor)");
                let status = sensor_status(sensor, report.mode);
                if !quiet || !matches!(status, "PASS" | "WARN") {
                    println!("  {name}: {status}");
                }
            }
        }
        if !workspace.stderr.is_empty() {
            eprint!("{}", workspace.stderr);
            if !workspace.stderr.ends_with('\n') {
                eprintln!();
            }
        }
    }
    if !quiet || !report.ok {
        let status = match (report.mode, report.exit_code) {
            ("simulate", _) => "PLAN",
            ("dry-run", _) => "DRY-RUN",
            (_, 0) => "PASS",
            (_, 1) => "FAIL",
            (_, 2) => "UNJUDGED",
            _ => "RUNNING",
        };
        println!("overall: {status} (exit {})", report.exit_code);
    }
}

#[cfg(test)]
mod tests {
    use super::{WorkspaceResult, aggregate_exit_code};

    fn result(exit_code: i32) -> WorkspaceResult {
        WorkspaceResult {
            root: "workspace".to_owned(),
            status: "failed",
            ok: exit_code == 0,
            exit_code,
            report: None,
            stderr: String::new(),
            reason: None,
        }
    }

    #[test]
    fn mixed_failure_and_unjudged_workspaces_preserve_verify_exit_priority() {
        assert_eq!(aggregate_exit_code(&[result(1), result(2)]), 1);
        assert_eq!(aggregate_exit_code(&[result(3), result(1)]), 3);
    }
}
