//! `task_action` split out of `crates/do-harness/src/cli/actions.rs` to keep that file under the 500-line ceiling.
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
    /// Assemble a bounded, read-only resume context for a task.
    Context {
        /// Task id.
        #[arg(value_name = "ID")]
        id: i64,
        /// Output format.
        #[arg(long, value_enum, default_value_t = Format::Text)]
        format: Format,
        /// Maximum rendered bytes; task identity and the next action always
        /// survive, so a very small budget is a floor, not a truncation.
        #[arg(
            long,
            default_value_t = crate::task::context::DEFAULT_MAX_BYTES,
            value_name = "N"
        )]
        max_bytes: usize,
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
