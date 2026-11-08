//! Deterministic, sequential PR sweep (#471).
//!
//! `do-harness pr sweep` orchestrates the pr-triage skill's loop as one typed
//! command: inventory → authorize owners → no-effect → update base → wait for
//! CI → evidence snapshot → final readiness with a receipt → direct pinned
//! merge → post-merge main verification. The default is a read-only dry run;
//! `--execute` authorizes owned/bot branch updates and auto-merge disarm, and
//! `--execute --merge` additionally authorizes eligible merges and verified
//! closures. The deterministic path never calls a model.

pub mod machine;
pub mod plan;
pub mod report;
pub mod state;
pub mod step;

use std::path::Path;

use crate::CliError;
use crate::pr::gh;
use crate::report::Format;

use self::report::SweepReport;
use self::state::ModeRecord;

/// Arguments for `pr sweep` as parsed by the CLI.
#[derive(Debug, Clone, Copy)]
#[allow(clippy::struct_excessive_bools)]
pub struct SweepArgs {
    /// Sweep all open PRs.
    pub all: bool,
    /// Sweep exactly one PR.
    pub pr: Option<u64>,
    /// Read-only inventory and report (the default). Also set by the global
    /// `--dry-run`, which always wins over `--execute`/`--merge`.
    pub dry_run: bool,
    /// Authorize owned/bot branch updates and auto-merge disarm.
    pub execute: bool,
    /// Also authorize eligible merges and closures.
    pub merge: bool,
    /// Wait bound (seconds) for CI and post-merge verification.
    pub wait: u64,
    /// Output format.
    pub format: Format,
}

/// Runs a sweep and prints the schema-1 report.
///
/// # Errors
///
/// Returns [`CliError::Usage`] for invalid argument combinations and for any
/// halt with indeterminate evidence; [`CliError::Verify`] for a post-merge
/// regression halt. Per-PR blockers are reported in the report, not as
/// process failures.
#[allow(clippy::too_many_lines)]
pub fn run(root: &Path, args: SweepArgs) -> Result<(), CliError> {
    if args.all == args.pr.is_some() {
        return Err(CliError::Usage(anyhow::anyhow!(
            "pr sweep requires exactly one target: --all or --pr N"
        )));
    }
    if args.merge && !args.execute {
        return Err(CliError::Usage(anyhow::anyhow!(
            "--merge requires --execute"
        )));
    }
    // `--dry-run` is a hard stop: it overrides the mutating flags instead of
    // being silently ignored when they are also present.
    let execute = args.execute && !args.dry_run;
    let merge = args.merge && !args.dry_run;
    let mode = ModeRecord {
        dry_run: !execute,
        execute,
        merge,
    };

    // LIST: one read-only inventory.
    let entries = if let Some(number) = args.pr {
        vec![gh::pr_entry_view(root, number).map_err(CliError::Usage)?]
    } else {
        gh::list_open_prs(root).map_err(CliError::Usage)?
    };
    let planned = plan::ordered_worklist(entries);

    let state_file = state::state_path(root);
    if let Some(parent) = state_file.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let mut sweep_state = state::SweepStateV1::load(&state_file);
    let open_numbers: Vec<u64> = planned.iter().map(|planned| planned.entry.number).collect();
    sweep_state.reconcile(root, &open_numbers);

    let mut drive_input = machine::DriveInput {
        mode: mode.clone(),
        wait_seconds: args.wait,
        state_path: &state_file,
        state: &mut sweep_state,
    };
    let output = machine::drive(root, &planned, &mut drive_input).map_err(CliError::Usage)?;

    let report = SweepReport {
        schema_version: report::SWEEP_REPORT_SCHEMA_VERSION,
        mode,
        target: report::TargetRecord {
            all: args.all,
            pr: args.pr,
        },
        wait_seconds: args.wait,
        zero_model: true,
        state_path: state_file.display().to_string(),
        prs: output.prs,
        summary: output.summary,
        halted: output.halted,
    };
    report.render(args.format);

    match report.halted.as_ref().map(|halt| halt.code.as_str()) {
        Some("regression") => Err(CliError::Verify(anyhow::anyhow!(
            "post-merge main regression; sweep halted"
        ))),
        Some("unknown") => Err(CliError::Usage(anyhow::anyhow!(
            "post-merge main verification unknown at the deadline; sweep halted"
        ))),
        _ => Ok(()),
    }
}

/// Ensures the module is reachable from the crate root.
#[allow(dead_code)]
fn _assert_dispatch() {
    let _ = run as fn(&Path, SweepArgs) -> Result<(), CliError>;
}
