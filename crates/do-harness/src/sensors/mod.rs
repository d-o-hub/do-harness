//! Computational sensor runner for `do-harness verify`.

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use anyhow::{Result, anyhow};

use crate::config::{Config, SensorSpec};
use crate::report::{Format, SensorResult, VerifyReport};

/// Builds the halted-by-policy result for a sensor the fail-fast guard
/// refuses to execute (ok=false, no exit code, zero duration).
pub(crate) fn sensor_blocked(spec: &SensorSpec) -> SensorResult {
    use crate::telemetry::FAIL_FAST_STRIKES;
    exec::sensor_result(
        spec,
        false,
        None,
        0,
        format!(
            "halted: sensor '{}' has failed {} consecutive times; resolve the underlying issue before re-running",
            spec.name, FAIL_FAST_STRIKES
        ),
    )
}

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
    /// Maximum sensors in flight; `None` falls back to `jobs` in
    /// `do-harness.toml`, then to sequential execution.
    pub jobs: Option<usize>,
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

    let jobs = parallel::effective_jobs(cfg.jobs, opts.jobs)?;
    let cancel = AtomicBool::new(false);
    let results = parallel::run_parallel(selection.specs, root, opts, jobs, &cancel);

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

#[cfg(test)]
mod tests;

mod exec;
mod parallel;
