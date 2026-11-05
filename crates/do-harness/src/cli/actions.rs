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

#[path = "pr_actions.rs"]
mod pr_actions;
pub use pr_actions::PrAction;

/// Available `metrics` report scopes.
#[derive(Debug, Subcommand)]
pub enum MetricsAction {
    /// PR-loop measures: time-to-green, pushes per green, waivers, comments, cancelled runs, reruns.
    Pr {
        /// Repository as OWNER/NAME (any repository the caller can read).
        #[arg(long, value_name = "OWNER/NAME")]
        repo: String,
        /// Window: a duration (`30d`, `12h`, `2w`) or a date (`YYYY-MM-DD`).
        #[arg(long, value_name = "WINDOW", default_value = "30d")]
        since: String,
        /// Maximum pull requests measured, newest first.
        #[arg(long, value_name = "N", default_value_t = 30)]
        limit: usize,
        /// Ignore the cached snapshot and refetch.
        #[arg(long)]
        recompute: bool,
        /// Output format.
        #[arg(long, value_enum, default_value_t = Format::Text)]
        format: Format,
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
        /// Filter by workstream scope (`branch:<name>`, `task:<id>`, `global`,
        /// or `all` for every workstream).
        #[arg(long, value_name = "SCOPE", conflicts_with = "task")]
        scope: Option<String>,
        /// Output format.
        #[arg(long, value_enum, default_value_t = Format::Text)]
        format: Format,
    },
    /// Clear fail-fast error signatures (`--sensor sensor:<name>`).
    Clear {
        /// Only clear this signature key (e.g. `sensor:<name>`).
        #[arg(long, value_name = "SENSOR")]
        sensor: Option<String>,
        /// Only clear signatures for this task id.
        #[arg(long, value_name = "ID")]
        task: Option<i64>,
        /// Only clear signatures for this workstream scope (`branch:<name>`,
        /// `task:<id>`, `global`, or `all` for every workstream).
        #[arg(long, value_name = "SCOPE", conflicts_with = "task")]
        scope: Option<String>,
        /// Allow clearing every workstream without a filter.
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

/// Available progressive-disclosure skill actions.
#[derive(Debug, Subcommand)]
pub enum SkillsAction {
    /// Rank skills by relevance to a query using metadata only.
    Suggest {
        /// Task description to match against skill metadata.
        #[arg(long, value_name = "TEXT")]
        query: String,
        /// Maximum number of candidates to return.
        #[arg(long, default_value_t = 5, value_name = "N")]
        limit: usize,
        /// Output format.
        #[arg(long, value_enum, default_value_t = Format::Text)]
        format: Format,
    },
    /// Check manifest-managed shared skills against their pinned digests.
    Drift {
        /// Manifest to read; defaults to `.agents/skills-manifest.toml`.
        #[arg(long, value_hint = ValueHint::FilePath, value_name = "FILE")]
        manifest: Option<PathBuf>,
        /// Output format.
        #[arg(long, value_enum, default_value_t = Format::Text)]
        format: Format,
    },
    /// Replace only the installed skill-creator structure validator.
    RefreshValidator,
}
/// Available impact-gated feedback actions.
#[derive(Debug, Subcommand)]
pub enum FeedbackAction {
    /// Generate a local feedback draft from recurring or severe failure evidence.
    Draft {
        /// Target repository (`OWNER/REPO`).
        #[arg(long, value_name = "OWNER/REPO")]
        repo: String,
        /// Historical search window in days (default: 30).
        #[arg(long, value_name = "DAYS")]
        days: Option<i64>,
        /// Minimum independent attributable fires required (default: 3).
        #[arg(long, value_name = "N")]
        min_fires: Option<u32>,
        /// Write draft to specific file path.
        #[arg(long, value_hint = ValueHint::FilePath, value_name = "FILE")]
        out: Option<PathBuf>,
        /// Output format.
        #[arg(long, value_enum, default_value_t = Format::Text)]
        format: Format,
    },
    /// Explicitly submit a feedback draft.
    Submit {
        /// Path to the prepared draft JSON file.
        #[arg(long, value_hint = ValueHint::FilePath, value_name = "FILE")]
        draft: PathBuf,
        /// Target repository override.
        #[arg(long, value_name = "OWNER/REPO")]
        repo: Option<String>,
        /// Skip interactive confirmation (unattended mode).
        #[arg(long)]
        yes: bool,
        /// Perform search and preview without creating an issue.
        #[arg(long)]
        dry_run: bool,
    },
    /// Fully automated end-to-end feedback generation, deduplication, and submission.
    Auto {
        /// Target repository (`OWNER/REPO`).
        #[arg(long, value_name = "OWNER/REPO")]
        repo: String,
        /// Historical search window in days (default: 30).
        #[arg(long, value_name = "DAYS")]
        days: Option<i64>,
        /// Minimum independent attributable fires required (default: 3).
        #[arg(long, value_name = "N")]
        min_fires: Option<u32>,
        /// Perform candidate generation and deduplication search without publishing.
        #[arg(long)]
        dry_run: bool,
        /// Output format.
        #[arg(long, value_enum, default_value_t = Format::Text)]
        format: Format,
    },
}

/// Available doctor actions.
#[derive(Debug, Subcommand)]
pub enum DoctorAction {
    /// Check repository PR merge policy against configured expectations.
    PrPolicy {
        /// Output format.
        #[arg(long, value_enum)]
        format: Option<Format>,
    },
}
