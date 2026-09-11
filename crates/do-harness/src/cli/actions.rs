//! Subcommand action enums for `do-harness task`, `errors`, `trace`, and `hook`.
//!
//! Split from `cli.rs` to keep that file under the modularity cap.

use std::path::PathBuf;

use clap::{Subcommand, ValueHint};

use crate::report::Format;

/// Available task-state actions.
#[derive(Debug, Subcommand)]
pub enum TaskAction {
    /// Write the task list to plans/tasks.json or specified output.
    Export {
        /// Output file path or - for stdout.
        #[arg(long, short, value_hint = ValueHint::FilePath, value_name = "FILE")]
        output: Option<PathBuf>,
        /// Write directly to stdout instead of file.
        #[arg(long)]
        stdout: bool,
        /// Output format (json/text).
        #[arg(long, value_enum, default_value_t = Format::Json)]
        format: Format,
    },
    /// Validate plans/tasks.json against the state database.
    Import {
        /// Snapshot file (defaults to plans/tasks.json under the root).
        #[arg(long, value_hint = ValueHint::FilePath, value_name = "FILE")]
        file: Option<PathBuf>,
        /// Exit non-zero when the snapshot and database drift.
        #[arg(long)]
        check: bool,
    },
    /// Print tasks from the state database.
    List {
        /// Filter tasks by status (pending, `in_progress`, completed, failed).
        #[arg(long)]
        status: Option<String>,
        /// Filter tasks by method name.
        #[arg(long)]
        method: Option<String>,
        /// Filter tasks by parent ID.
        #[arg(long, value_name = "ID")]
        parent: Option<i64>,
        /// Output format.
        #[arg(long, value_enum, default_value_t = Format::Text)]
        format: Format,
    },
    /// Show details for a specific task.
    Show {
        /// Task id.
        #[arg(value_name = "ID")]
        id: i64,
        /// Output format.
        #[arg(long, value_enum, default_value_t = Format::Text)]
        format: Format,
    },
    /// Add a task in `pending` state.
    Add {
        /// Human-readable task title.
        title: String,
        /// Name of the HTN method this task follows (defined in plans/methods.json).
        #[arg(long, help = "Name of HTN method defined in plans/methods.json")]
        method: Option<String>,
        /// Parent task id.
        #[arg(long, value_name = "ID")]
        parent: Option<i64>,
        /// Recorded precondition guard.
        #[arg(long)]
        precondition: Option<String>,
    },
    /// Advance the task's subtask pointer.
    Advance {
        /// Task id.
        #[arg(value_name = "ID")]
        id: i64,
        /// Perform dry run without state changes.
        #[arg(long)]
        dry_run: bool,
    },
    /// Mark a task done once its sensor-gated subtasks have passed.
    Done {
        /// Task id.
        #[arg(value_name = "ID")]
        id: i64,
        /// Perform dry run without state changes.
        #[arg(long)]
        dry_run: bool,
    },
    /// Mark a task failed.
    Fail {
        /// Task id.
        #[arg(value_name = "ID")]
        id: i64,
    },
    /// Remove or cancel a task.
    Remove {
        /// Task id.
        #[arg(value_name = "ID")]
        id: i64,
    },
}

/// Available error-signature actions.
#[derive(Debug, Subcommand)]
pub enum ErrorsAction {
    /// List fail-fast error signatures (prefixed with `sensor:`).
    List {
        /// Scope to one task id.
        #[arg(long, value_name = "ID")]
        task: Option<i64>,
        /// Output format.
        #[arg(long, value_enum, default_value_t = Format::Text)]
        format: Format,
    },
    /// Clear fail-fast error signatures (e.g. `sensor:<name>`).
    Clear {
        /// Only clear this signature key (e.g. `sensor:<name>`).
        #[arg(long, value_name = "SENSOR")]
        sensor: Option<String>,
        /// Only clear signatures for this task id.
        #[arg(long, value_name = "ID")]
        task: Option<i64>,
        /// Force clearing without prompt.
        #[arg(long)]
        force: bool,
        /// Perform dry run without clearing.
        #[arg(long)]
        dry_run: bool,
    },
}

/// Available trace actions.
#[derive(Debug, Subcommand)]
pub enum TraceAction {
    /// Record a trace of an executed command and its resolution.
    Add {
        /// Session identifier grouping related traces.
        #[arg(long)]
        session: String,
        /// Owning task id.
        #[arg(long, value_name = "ID")]
        task: Option<i64>,
        /// The command that was executed.
        #[arg(long)]
        command: Option<String>,
        /// Error diff or failure output captured.
        #[arg(long = "error-diff")]
        error_diff: Option<String>,
        /// Steps taken to resolve the failure.
        #[arg(long = "resolution-steps")]
        resolution_steps: Option<String>,
    },
    /// Print traces for a session.
    List {
        /// Session identifier.
        #[arg(long)]
        session: String,
        /// Output format.
        #[arg(long, value_enum, default_value_t = Format::Text)]
        format: Format,
    },
    /// List distinct trace session identifiers.
    Sessions {
        /// Output format.
        #[arg(long, value_enum, default_value_t = Format::Text)]
        format: Format,
    },
}

/// Available hook-management actions.
#[derive(Debug, Copy, Clone, Subcommand)]
pub enum HookAction {
    /// Write pre-commit and pre-push hooks into `.git/hooks/`.
    Install {
        /// Overwrite foreign (unmanaged) hook files.
        #[arg(long)]
        force: bool,
    },
    /// Remove managed hooks, leaving foreign hook files untouched.
    Uninstall,
    /// Show whether the managed hooks and release binary are present.
    Status {
        /// Output format.
        #[arg(long, value_enum, default_value_t = Format::Text)]
        format: Format,
    },
    /// Show diff between installed hooks and current templates.
    Diff,
}
