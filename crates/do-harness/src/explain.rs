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
    /// `all` | `working-tree` | `range` | `working-tree+range`.
    pub selection_mode: String,
    /// `--since` ref as requested, when range selection was used.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub since: Option<String>,
    /// Commit the requested ref resolved to.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_sha: Option<String>,
    /// `HEAD` commit the range was compared against.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub head_sha: Option<String>,
    /// Merge base the range diff started from.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub merge_base_sha: Option<String>,
    /// Why discovery failed closed, when it did.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub diagnostic: Option<String>,
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
    since: Option<&str>,
) -> std::result::Result<ExplainReport, anyhow::Error> {
    let candidates: Vec<&config::SensorSpec> = crate::signals::candidate_specs(cfg, set)?;
    let selection_input = crate::changes::discover_selection(root, changed, since);
    let (selection, changed_files) = if changed || since.is_some() {
        let files = selection_input
            .paths()
            .iter()
            .map(ToString::to_string)
            .collect();
        (
            crate::applicability::select(&candidates, &selection_input.changed),
            files,
        )
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
    let range = selection_input.range.as_ref();
    Ok(ExplainReport {
        set: set.map(ToOwned::to_owned),
        selection_mode: selection_input.mode.as_str().to_owned(),
        since: range.map(|r| r.requested.clone()),
        base_sha: range.map(|r| r.base_sha.clone()),
        head_sha: range.map(|r| r.head_sha.clone()),
        merge_base_sha: range.map(|r| r.merge_base_sha.clone()),
        diagnostic: selection_input.diagnostic.clone(),
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
    since: Option<String>,
    format: Format,
) -> std::result::Result<(), CliError> {
    let cfg = config::load(root, config_path)
        .await
        .map_err(CliError::Usage)?;
    let report = explain_report(root, &cfg, set.as_deref(), changed, since.as_deref())
        .map_err(CliError::Usage)?;
    match format {
        Format::Text => {
            println!("selection: {}", report.selection_mode);
            if let Some(since) = &report.since {
                println!("since: {since}");
            }
            if let Some(base) = &report.base_sha {
                println!("base: {base}");
            }
            if let Some(head) = &report.head_sha {
                println!("head: {head}");
            }
            if let Some(merge_base) = &report.merge_base_sha {
                println!("merge-base: {merge_base}");
            }
            if let Some(diagnostic) = &report.diagnostic {
                println!("selection failed closed: {diagnostic}");
            }
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
