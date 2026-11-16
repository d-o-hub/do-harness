//! `do-harness verify --simulate`: the execution plan, never the execution.
//!
//! `explain` answers *which* sensors a change selects; simulate answers *what
//! running them would do*: the exact `argv`, the signal sets each sensor
//! belongs to, its `when-changed` globs, timeout, retry budget and severity.
//! An agent can price a run before spending it, and a CI job can publish the
//! plan without touching the workspace.
//!
//! Nothing here executes a sensor, reads beat state, or writes evidence. The
//! plan is derived from the same `resolve_selection` call `verify` uses, so a
//! plan cannot disagree with the run it describes for the same inputs.

use std::path::Path;

use serde::Serialize;

use crate::config::{Config, SensorSpec};
use crate::report::VerifyFormat;
use crate::sensors::ResolvedSelection;

/// Schema version of the simulate document.
///
/// Bumped when a field is removed or its meaning changes; consumers may pin
/// against it the same way they pin the verify JSON contract.
pub const PLAN_SCHEMA_VERSION: u32 = 1;

/// One sensor the plan would run.
#[derive(Debug, Clone, Serialize)]
pub struct PlannedSensor {
    /// Configured sensor name.
    pub name: String,
    /// Program and arguments exactly as `verify` would spawn them.
    pub argv: Vec<String>,
    /// Development signal sets that contain this sensor, sorted.
    pub sets: Vec<String>,
    /// `when-changed` globs; empty means always applicable.
    pub when_changed: Vec<String>,
    /// Wall-clock budget in seconds (`null` means the built-in default).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout_secs: Option<u64>,
    /// Retry attempts configured for a failing run.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry: Option<u32>,
    /// Effective gate severity.
    pub severity: &'static str,
    /// Why applicability selected this sensor (or the unfiltered reason).
    pub selected_because: String,
}

/// One candidate the plan would skip, with its reason.
#[derive(Debug, Clone, Serialize)]
pub struct PlannedSkip {
    /// Configured sensor name.
    pub name: String,
    /// Applicability reason from the same selection `verify` would use.
    pub reason: String,
}

/// The machine-readable plan (`verify --simulate --format json`).
#[derive(Debug, Clone, Serialize)]
pub struct SimulateReport {
    /// Schema version of this document.
    pub schema_version: u32,
    /// Workspace root the run would use.
    pub root: String,
    /// Requested signal set; `null` when the plan covers every sensor.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub set: Option<String>,
    /// `all` | `working-tree` | `range` | `working-tree+range`.
    pub selection_mode: String,
    /// `--since` ref as requested, when range selection applies.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub since: Option<String>,
    /// Commit the requested ref resolved to.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_sha: Option<String>,
    /// `HEAD` commit the range would be compared against.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub head_sha: Option<String>,
    /// Why change discovery failed when it did; selection is fail-closed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub discovery_error: Option<String>,
    /// Sensors in the order `verify` would run them.
    pub sensors: Vec<PlannedSensor>,
    /// Candidate sensors the plan would skip, in candidate order.
    pub skipped: Vec<PlannedSkip>,
    /// The order the plan would execute, mirrored for consumers that want the
    /// schedule without reading `sensors`.
    pub estimated_order: Vec<String>,
    /// Facts a caller can rely on without inspecting the sensors.
    pub notes: Vec<String>,
}

/// Builds the plan from an already-resolved selection.
#[must_use]
pub fn plan(
    root: &Path,
    cfg: &Config,
    selection: &ResolvedSelection<'_>,
    set: Option<&str>,
) -> SimulateReport {
    let range = selection.changes.range.as_ref();
    let sensors: Vec<PlannedSensor> = selection
        .specs
        .iter()
        .map(|spec| planned(cfg, spec, selection))
        .collect();
    SimulateReport {
        schema_version: PLAN_SCHEMA_VERSION,
        root: root.display().to_string(),
        set: set.map(ToOwned::to_owned),
        selection_mode: selection.changes.mode.as_str().to_owned(),
        since: range.map(|resolved| resolved.requested.clone()),
        base_sha: range.map(|resolved| resolved.base_sha.clone()),
        head_sha: range.map(|resolved| resolved.head_sha.clone()),
        discovery_error: selection.discovery_error.clone(),
        estimated_order: sensors.iter().map(|sensor| sensor.name.clone()).collect(),
        sensors,
        skipped: selection
            .skipped
            .iter()
            .map(|skipped| PlannedSkip {
                name: skipped.name.clone(),
                reason: skipped.reason.clone(),
            })
            .collect(),
        notes: vec![
            "no sensor was executed".to_owned(),
            "no evidence or state was written".to_owned(),
        ],
    }
}

/// Builds one planned sensor, with the reason applicability selected it.
fn planned(cfg: &Config, spec: &SensorSpec, selection: &ResolvedSelection<'_>) -> PlannedSensor {
    let selected_because = selection
        .selected
        .iter()
        .find(|selected| selected.name == spec.name)
        .map_or_else(
            || "selected by signal set".to_owned(),
            |selected| selected.reason.clone(),
        );
    PlannedSensor {
        name: spec.name.clone(),
        argv: spec.argv.clone(),
        sets: cfg
            .signal_sets
            .iter()
            .filter(|(_, names)| names.contains(&spec.name))
            .map(|(set, _)| set.clone())
            .collect(),
        when_changed: spec.when_changed.clone(),
        timeout_secs: spec.timeout,
        retry: spec.retry,
        severity: match spec.effective_severity() {
            crate::config::SensorSeverity::Error => "error",
            crate::config::SensorSeverity::Warn => "warn",
        },
        selected_because,
    }
}

/// Prints the plan in the requested format.
///
/// `JUnit` is not a plan format: the CLI rejects it before reaching here, and
/// every other value renders the same information a consumer needs.
///
/// # Errors
///
/// Returns an error when the plan cannot be serialized as JSON.
pub fn print(report: &SimulateReport, format: VerifyFormat) -> anyhow::Result<()> {
    if format == VerifyFormat::Json {
        println!("{}", crate::report::json_pretty(report)?);
        return Ok(());
    }
    println!("plan ({} sensors, nothing executed):", report.sensors.len());
    println!("selection: {}", report.selection_mode);
    if let Some(since) = &report.since {
        println!("since: {since}");
    }
    for sensor in &report.sensors {
        let timeout = sensor
            .timeout_secs
            .map_or_else(|| "default".to_owned(), |seconds| format!("{seconds}s"));
        let sets = if sensor.sets.is_empty() {
            "no signal set".to_owned()
        } else {
            sensor.sets.join(",")
        };
        println!(
            "  {} [{}] severity={} timeout={timeout} argv={}",
            sensor.name,
            sets,
            sensor.severity,
            sensor.argv.join(" ")
        );
        if !sensor.when_changed.is_empty() {
            println!("      when-changed: {}", sensor.when_changed.join(", "));
        }
    }
    for skipped in &report.skipped {
        println!("  skip {} ({})", skipped.name, skipped.reason);
    }
    for note in &report.notes {
        println!("{note}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use crate::config::{Config, HooksConfig};

    fn spec(name: &str) -> SensorSpec {
        SensorSpec {
            kind: None,
            fix: None,
            name: name.to_owned(),
            argv: vec!["true".to_owned()],
            retry: None,
            timeout_justification: None,
            max_output_bytes: None,
            environment_allowlist: None,
            redact_env: Vec::new(),
            timeout: None,
            severity: None,
            allow_failure: false,
            transient_exit_codes: vec![],
            artifacts: Vec::new(),
            environment_inputs: Vec::new(),
            coverage_inputs: Vec::new(),
            inputs: Vec::new(),
            when_changed: vec![],
        }
    }

    fn config_with(sensors: Vec<SensorSpec>, sets: &[(&str, &[&str])]) -> Config {
        Config {
            language: None,
            hooks: HooksConfig {
                pre_commit: vec![],
                pre_push: vec![],
            },
            signal_sets: sets
                .iter()
                .map(|(set, names)| {
                    (
                        (*set).to_owned(),
                        names.iter().map(|name| (*name).to_owned()).collect(),
                    )
                })
                .collect(),
            sensors,
            jobs: None,
            repairs: Vec::new(),
            pr: crate::pr::author_policy::PrConfig::default(),
        }
    }

    #[test]
    fn a_plan_reports_sets_timeout_and_order() {
        let mut sensors = vec![spec("fmt"), spec("check")];
        sensors[0].timeout = Some(30);
        let cfg = config_with(sensors, &[("fast", &["fmt", "check"])]);
        let selection = crate::sensors::ResolvedSelection {
            specs: vec![&cfg.sensors[0], &cfg.sensors[1]],
            selected: vec![crate::applicability::Selected {
                name: "fmt".to_owned(),
                reason: "matched src/**".to_owned(),
            }],
            skipped: vec![],
            changes: crate::changes::ChangeSelection::default(),
            discovery_error: None,
        };
        let report = plan(Path::new("/tmp/root"), &cfg, &selection, None);
        assert_eq!(report.schema_version, PLAN_SCHEMA_VERSION);
        assert_eq!(report.estimated_order, ["fmt", "check"]);
        assert_eq!(report.sensors[0].sets, ["fast"]);
        assert_eq!(report.sensors[0].timeout_secs, Some(30));
        assert_eq!(report.sensors[0].selected_because, "matched src/**");
        assert!(
            report
                .notes
                .iter()
                .any(|note| note.contains("no sensor was executed")),
            "{:?}",
            report.notes
        );
    }

    #[test]
    fn an_empty_plan_still_documents_that_nothing_ran() {
        let cfg = config_with(Vec::new(), &[]);
        let selection = crate::sensors::ResolvedSelection {
            specs: vec![],
            selected: vec![],
            skipped: vec![],
            changes: crate::changes::ChangeSelection::default(),
            discovery_error: None,
        };
        let report = plan(Path::new("/tmp/root"), &cfg, &selection, None);
        assert!(report.sensors.is_empty());
        assert_eq!(report.estimated_order.len(), 0);
        // A consumer must never read "no sensors" as "nothing to run here":
        // the notes are the only signal that this was a plan, not a run.
        assert_eq!(report.notes.len(), 2);
    }
}
