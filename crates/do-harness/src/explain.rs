//! `do-harness explain`: why each sensor is selected or skipped.
//!
//! Explain runs the same change-aware selection as `verify --changed`
//! without executing any sensor, so an agent runtime can see deterministically
//! which checks apply to the current change and why.

use std::path::Path;

use serde::Serialize;

use crate::applicability::{Selected, Selection, Skipped};
use crate::report::Format;
use crate::{CliError, config};

/// Machine-readable applicability report (the `--format json` contract).
#[derive(Debug, Clone, Serialize)]
pub struct ExplainReport {
    /// Requested signal set; `None` means the effective sensor list.
    pub set: Option<String>,
    /// Changed repository-relative paths driving selection, sorted.
    pub changed_files: Vec<String>,
    /// Applicable sensors with reasons, in candidate order.
    pub selected: Vec<Selected>,
    /// Inapplicable sensors with reasons, in candidate order.
    pub skipped: Vec<Skipped>,
}

/// Builds the applicability report without executing any sensor.
///
/// # Errors
///
/// Returns a usage error when the requested signal set is unknown.
pub fn explain_report(
    root: &Path,
    cfg: &config::Config,
    set: Option<&str>,
    changed: bool,
) -> std::result::Result<ExplainReport, anyhow::Error> {
    let candidates: Vec<&config::SensorSpec> = crate::signals::candidate_specs(cfg, set)?;
    let (selection, changed_files) = if changed {
        let changed = crate::changes::discover(root);
        let files = changed.paths().iter().map(ToString::to_string).collect();
        (crate::applicability::select(&candidates, &changed), files)
    } else {
        let selected = candidates
            .iter()
            .map(|spec| Selected {
                name: spec.name.clone(),
                reason: crate::applicability::UNFILTERED_REASON.to_owned(),
            })
            .collect();
        (
            Selection {
                selected,
                skipped: Vec::new(),
            },
            Vec::new(),
        )
    };
    Ok(ExplainReport {
        set: set.map(ToOwned::to_owned),
        changed_files,
        selected: selection.selected,
        skipped: selection.skipped,
    })
}

/// Runs the `explain` subcommand: selection reasoning, no sensor execution.
pub(crate) async fn run(
    root: &Path,
    config_path: Option<&Path>,
    set: Option<String>,
    changed: bool,
    format: Format,
) -> std::result::Result<(), CliError> {
    let cfg = config::load(root, config_path)
        .await
        .map_err(CliError::Usage)?;
    let report = explain_report(root, &cfg, set.as_deref(), changed).map_err(CliError::Usage)?;
    match format {
        Format::Text => {
            if !report.changed_files.is_empty() {
                println!("changed files:");
                for file in &report.changed_files {
                    println!("  {file}");
                }
            }
            println!("selected:");
            for sensor in &report.selected {
                println!("  {} ({})", sensor.name, sensor.reason);
            }
            println!("skipped:");
            for sensor in &report.skipped {
                println!("  {} ({})", sensor.name, sensor.reason);
            }
        }
        Format::Json => match serde_json::to_writer_pretty(std::io::stdout(), &report) {
            Ok(()) => println!(),
            Err(err) => eprintln!("error: failed to serialize report: {err}"),
        },
    }
    Ok(())
}
