//! Flattened argument groups for the subcommands whose flags would otherwise
//! push `command.rs` over the 500-line ceiling.
//!
//! Each struct is `clap::Args`, so the CLI surface is unchanged: the parent
//! variant flattens the group exactly as it flattened the inline fields.

use std::path::PathBuf;

use clap::{ArgAction, Subcommand, ValueHint};

use crate::report::Format;

/// Arguments for `migrate` (flattened into the `migrate` subcommand).
#[derive(Debug, clap::Args)]
pub struct MigrateArgs {
    /// Preview the required changes without writing (the default).
    #[arg(long, conflicts_with = "write")]
    pub diff: bool,
    /// Apply the changes; refuses conflicts it cannot resolve.
    #[arg(long)]
    pub write: bool,
    /// Output format.
    #[arg(long, value_enum, default_value_t = crate::report::Format::Text)]
    pub format: crate::report::Format,
}

/// Arguments for `report` (flattened into the `report` subcommand).
#[derive(Debug, clap::Args)]
pub struct ReportArgs {
    /// Output format.
    #[arg(long, value_enum, default_value_t = crate::reports::ReportFormat::Text)]
    pub format: crate::reports::ReportFormat,
    /// Filter by sensor name.
    #[arg(long, value_name = "SENSOR")]
    pub sensor: Option<String>,
    /// Inclusive window start as a Unix timestamp in seconds.
    #[arg(long, value_name = "UNIX_SECONDS")]
    pub since: Option<String>,
    /// Inclusive window end as a Unix timestamp in seconds.
    #[arg(long, value_name = "UNIX_SECONDS")]
    pub until: Option<String>,
    /// Filter by workstream scope (e.g. `branch:main`, `task:1`, `global`, or `all`).
    /// Defaults to the current git branch.
    #[arg(long, value_name = "SCOPE")]
    pub scope: Option<String>,
    /// Filter by task id (equivalent to `--scope task:<ID>`).
    #[arg(long, value_name = "ID", conflicts_with = "scope")]
    pub task: Option<i64>,
    /// Filter by branch name (equivalent to `--scope branch:<NAME>`).
    #[arg(long, value_name = "BRANCH", conflicts_with = "scope")]
    pub branch: Option<String>,
    /// Aggregate across all workstreams and scopes.
    #[arg(long, conflicts_with_all = ["scope", "task", "branch"])]
    pub all: bool,
}

/// Arguments for `lint` (flattened into a tuple variant).
#[derive(Debug, clap::Args)]
pub struct LintArgs {
    /// Output format.
    #[arg(long, value_enum, default_value_t = crate::report::Format::Text)]
    pub format: crate::report::Format,
}

/// Arguments for `sensor new`.
#[derive(Debug, clap::Args)]
pub struct NewSensorArgs {
    /// New sensor name (lowercase ASCII letters, digits, '-' and '_').
    pub name: String,
    /// One argv element; repeat once for the program and once per argument.
    #[arg(
        long = "arg",
        value_name = "ARG",
        required = true,
        allow_hyphen_values = true
    )]
    pub args: Vec<String>,
    /// Restrict the sensor to changes matching this glob; repeatable.
    #[arg(long = "when-changed", value_name = "GLOB")]
    pub when_changed: Vec<String>,
    /// Add the sensor to this already-declared signal set.
    #[arg(long, value_name = "SET")]
    pub set: Option<String>,
    /// Scaffold an executable stub script at this path (never overwrites).
    #[arg(long, value_hint = ValueHint::FilePath, value_name = "PATH")]
    pub script: Option<PathBuf>,
}

/// Arguments for the `sensor` command group (flattened into a tuple variant).
#[derive(Debug, clap::Args)]
pub struct SensorArgs {
    /// Sensor subcommand.
    #[command(subcommand)]
    pub action: SensorAction,
}

/// Available sensor actions.
#[derive(Debug, Subcommand)]
pub enum SensorAction {
    /// Add a validated `[[sensors]]` entry to do-harness.toml.
    New(NewSensorArgs),
}

/// Arguments for `list` (flattened into a tuple variant).
#[derive(Debug, clap::Args)]
pub struct ListArgs {
    /// List development signal-set names instead of sensors.
    #[arg(long)]
    pub sets: bool,
    /// Output format.
    #[arg(long, value_enum, default_value_t = Format::Text)]
    pub format: Format,
}

/// Arguments for `verify` (flattened into the `verify` subcommand).
#[derive(Debug, clap::Args)]
#[allow(clippy::struct_excessive_bools)] // independent CLI flags, not a state machine
pub struct VerifyArgs {
    /// Halt at the first failing sensor.
    #[arg(long)]
    pub fail_fast: bool,
    /// Output format.
    #[arg(long, value_enum, default_value_t = crate::report::VerifyFormat::Text)]
    pub format: crate::report::VerifyFormat,
    /// Print the execution plan and exit without spawning a sensor or
    /// writing evidence (a pre-flight for agents and CI cost checks).
    #[arg(long, conflicts_with_all = ["record", "bless", "evidence"])]
    pub simulate: bool,
    /// Run only the sensors in this development signal set.
    #[arg(long, value_name = "SET")]
    pub set: Option<String>,
    /// Run only sensors applicable to the working-tree change.
    #[arg(long)]
    pub changed: bool,
    /// Run only sensors applicable to the committed range
    /// `merge-base(<REF>, HEAD)..HEAD`; combine with `--changed` to union
    /// committed and uncommitted paths.
    #[arg(long, value_name = "REF")]
    pub since: Option<String>,
    /// Run only the named sensor (repeatable or comma-separated).
    #[arg(long = "only", action = ArgAction::Append, value_name = "SENSOR")]
    pub only: Vec<String>,
    /// Exclude named sensors from the run.
    #[arg(long = "exclude", action = ArgAction::Append, value_name = "SENSOR")]
    pub exclude: Vec<String>,
    /// Maximum workspace concurrency with `--workspaces`; otherwise the
    /// maximum sensors in flight (overrides `jobs` in do-harness.toml).
    #[arg(long, value_name = "N")]
    pub jobs: Option<usize>,
    /// Select workspace directories or configs matching this root-relative glob.
    #[arg(long, value_name = "GLOB")]
    pub workspaces: Option<String>,
    /// Reuse recorded passing beats for sensors whose declared inputs are
    /// unchanged: `warn` (default) executes and advises, `skip` reuses
    /// without executing (requires --record), `run` always executes.
    #[arg(long, value_enum, value_name = "MODE", default_value_t = crate::sensors::UnchangedMode::Warn)]
    pub unchanged: crate::sensors::UnchangedMode,
    /// Persist beats and error signatures into the state database.
    #[arg(long)]
    pub record: bool,
    /// Scope records and fail-fast strikes to this task id (or 'global').
    #[arg(long, requires = "record", value_name = "ID")]
    pub task: Option<String>,
    /// Record unscoped beats in the global namespace.
    #[arg(long, requires = "record", conflicts_with = "task")]
    pub global: bool,
    /// Write a machine-readable evidence artifact to path.
    #[arg(long, value_hint = ValueHint::FilePath, value_name = "FILE")]
    pub evidence: Option<PathBuf>,
    /// Enforce strong evidence: exit non-zero on skips or missing timing/exit codes.
    #[arg(long)]
    pub strict: bool,
    /// Lower or initialize blessed findings baselines from this run
    /// (requires --record; a bless never raises a baseline).
    #[arg(long, requires = "record")]
    pub bless: bool,
    /// Approver identity recorded with `--bless` (defaults to
    /// `DO_HARNESS_APPROVER` or the git user email).
    #[arg(long, requires = "bless", value_name = "NAME")]
    pub approver: Option<String>,
}
