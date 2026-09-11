//! CLI argument definitions for the do-harness harness.
//!
//! Extracted from `main.rs` to keep that file under the 500 LOC ceiling.

use std::path::PathBuf;

use clap::{ArgAction, Parser, Subcommand, ValueHint};

use crate::init;

mod actions;

use crate::report::Format;
pub use actions::{ErrorsAction, HookAction, PrAction, TaskAction, TraceAction};

/// Unified entrypoint for harness sensors and database maintenance.
#[derive(Debug, Parser)]
#[command(
    name = "do-harness",
    about = "do-harness agent execution harness CLI",
    version = crate::version::version_str(),
    propagate_version = true,
    arg_required_else_help = true,
    after_help = "Examples:\n  do-harness init\n  do-harness hook install\n  do-harness verify --record\n  do-harness verify --record --task <id>\n\nExit Codes:\n  0  Success\n  1  Verification or check failure\n  2  Usage, configuration, or environment error"
)]
pub struct Cli {
    /// Workspace root override (default: walk up from cwd).
    #[arg(long, global = true, value_hint = ValueHint::DirPath, value_name = "DIR")]
    pub root: Option<PathBuf>,
    /// Explicit path to do-harness.toml.
    #[arg(long, global = true, value_hint = ValueHint::FilePath, value_name = "FILE")]
    pub config: Option<PathBuf>,
    /// Verbosity level (-v, -vv).
    #[arg(short, long, global = true, action = ArgAction::Count)]
    pub verbose: u8,
    /// Suppress non-error messages.
    #[arg(short, long, global = true)]
    pub quiet: bool,
    /// Color output (auto, always, never).
    #[arg(long, global = true, value_name = "WHEN")]
    pub color: Option<String>,
    /// Default output file path.
    #[arg(long, global = true, value_hint = ValueHint::FilePath, value_name = "FILE")]
    pub output: Option<PathBuf>,
    /// Dry run without side effects.
    #[arg(long, global = true)]
    pub dry_run: bool,

    #[command(subcommand)]
    pub command: Command,
}

/// Available subcommands.
#[derive(Debug, Subcommand)]
pub enum Command {
    /// Print CLI version information.
    Version {
        /// Output format.
        #[arg(long, value_enum, default_value_t = Format::Text)]
        format: Format,
    },
    /// Run computational sensors.
    #[command(visible_alias = "check")]
    Verify {
        /// Halt at the first failing sensor.
        #[arg(long)]
        fail_fast: bool,
        /// Output format.
        #[arg(long, value_enum, default_value_t = Format::Text)]
        format: Format,
        /// Run only the sensors in this development signal set.
        #[arg(long, value_name = "SET")]
        set: Option<String>,
        /// Run only sensors applicable to the working-tree change.
        #[arg(long)]
        changed: bool,
        /// Run only the named sensor (repeatable or comma-separated).
        #[arg(long = "only", action = ArgAction::Append, value_name = "SENSOR")]
        only: Vec<String>,
        /// Exclude named sensors from the run.
        #[arg(long = "exclude", action = ArgAction::Append, value_name = "SENSOR")]
        exclude: Vec<String>,
        /// Persist beats and error signatures into the state database.
        #[arg(long)]
        record: bool,
        /// Scope records and fail-fast strikes to this task id.
        #[arg(long, requires = "record", value_name = "ID")]
        task: Option<i64>,
        /// Write a machine-readable evidence artifact to path.
        #[arg(long, value_hint = ValueHint::FilePath, value_name = "FILE")]
        evidence: Option<PathBuf>,
        /// Enforce strong evidence: exit non-zero on skips or missing timing/exit codes.
        #[arg(long)]
        strict: bool,
    },
    /// List sensor names.
    #[command(visible_alias = "ls")]
    List {
        /// List development signal-set names instead of sensors.
        #[arg(long)]
        sets: bool,
        /// Output format.
        #[arg(long, value_enum, default_value_t = Format::Text)]
        format: Format,
    },
    /// Explain which sensors the current change selects, without running them.
    Explain {
        /// Explain this development signal set (default: all sensors).
        #[arg(long, value_name = "SET")]
        set: Option<String>,
        /// Select by working-tree change instead of listing everything.
        #[arg(long)]
        changed: bool,
        /// Output format.
        #[arg(long, value_enum, default_value_t = Format::Text)]
        format: Format,
    },
    /// Report verification evidence freshness without running sensors.
    Status {
        /// Check evidence for this development signal set.
        #[arg(long, value_name = "SET")]
        set: Option<String>,
        /// Read evidence from this file instead of the default artifact.
        #[arg(long, value_hint = ValueHint::FilePath, value_name = "FILE")]
        evidence: Option<PathBuf>,
        /// Output format.
        #[arg(long, value_enum, default_value_t = Format::Text)]
        format: Format,
    },
    /// Deterministic PR analysis (read-only; works in any git repository).
    Pr {
        #[command(subcommand)]
        action: PrAction,
    },
    /// Apply pending database migrations.
    InitDb {
        /// Report pending migrations and exit non-zero when any are pending.
        #[arg(long)]
        check: bool,
        /// Report pending migrations without applying them (exit 0).
        #[arg(long)]
        dry_run: bool,
        /// Skip the interactive confirmation prompt.
        #[arg(long, short = 'y')]
        yes: bool,
    },
    /// Seed invariants from plans/invariants.json.
    Seed {
        /// Delete invariants no longer present in plans/invariants.json.
        #[arg(long)]
        prune: bool,
    },
    /// Scaffold a harness workspace in a target directory.
    Init {
        /// Language pack to scaffold (default: detect from the repository).
        #[arg(long, value_enum, value_name = "LANG")]
        language: Option<init::Language>,
        /// Overwrite existing files.
        #[arg(long)]
        force: bool,
        /// Output format.
        #[arg(long, value_enum, default_value_t = Format::Text)]
        format: Format,
        /// Do not seed invariants.
        #[arg(long)]
        no_seed: bool,
        /// Minimal setup without extra skills.
        #[arg(long)]
        minimal: bool,
        /// Skip creating/modifying .gitignore.
        #[arg(long)]
        no_gitignore: bool,
    },
    /// Inspect and export task state.
    Task {
        #[command(subcommand)]
        action: TaskAction,
    },
    /// Record and list interaction traces.
    Trace {
        #[command(subcommand)]
        action: TraceAction,
    },
    /// Extract a heuristic from a resolved trace.
    Distill {
        /// Skill the heuristic belongs to.
        #[arg(long, value_name = "SKILL")]
        skill: String,
        /// Generalized pattern to record.
        #[arg(long)]
        pattern: String,
        /// When the pattern applies.
        #[arg(long)]
        description: Option<String>,
        /// Source trace id; required as evidence of a resolved fix.
        #[arg(long = "from-trace", required = true, value_name = "ID")]
        from_trace: Option<i64>,
        /// Raise the skill's pass-rate bar after this recovery.
        #[arg(long = "to-fixture")]
        to_fixture: bool,
        /// Perform dry run without modifying skill files.
        #[arg(long)]
        dry_run: bool,
        /// Output format.
        #[arg(long, value_enum, default_value_t = Format::Text)]
        format: Format,
    },
    /// Inspect and clear fail-fast error signatures.
    Errors {
        #[command(subcommand)]
        action: ErrorsAction,
    },
    /// Validate skill structure and benchmark skill evals.
    Eval {
        /// Restrict evaluation to this skill directory name.
        #[arg(long, value_name = "SKILL")]
        skill: Option<String>,
        /// Bless a fully green run: re-baseline graders and update pass-rate floor.
        #[arg(
            long,
            help = "Re-baseline graders and update pass-rate floor on green run"
        )]
        bless: bool,
        /// List available skills.
        #[arg(long)]
        list_skills: bool,
        /// Halt on first failing evaluation.
        #[arg(long)]
        fail_fast: bool,
        /// Perform dry-run evaluation.
        #[arg(long)]
        dry_run: bool,
        /// Output format.
        #[arg(long, value_enum, default_value_t = Format::Text)]
        format: Format,
        /// Approver identity recorded with `--bless` (defaults to
        /// `DO_HARNESS_APPROVER` or the git user email).
        #[arg(long, value_name = "NAME")]
        approver: Option<String>,
    },
    /// Manage git hooks that run `do-harness verify`.
    Hook {
        #[command(subcommand)]
        action: HookAction,
    },
    /// Run diagnostic checks on binary resolution and git hook health.
    Doctor {
        /// Output format.
        #[arg(long, value_enum, default_value_t = Format::Text)]
        format: Format,
        /// Strictly enforce warning checks as failures.
        #[arg(long)]
        strict: bool,
    },
    /// Report harness trends: sensor stats, strikes, eval pass-rate history.
    Metrics {
        /// Output format.
        #[arg(long, value_enum, default_value_t = Format::Text)]
        format: Format,
        /// Filter by sensor name.
        #[arg(long, value_name = "SENSOR")]
        sensor: Option<String>,
        /// Filter by skill name.
        #[arg(long, value_name = "SKILL")]
        skill: Option<String>,
        /// Filter metrics since a Unix timestamp in seconds.
        #[arg(long, value_name = "UNIX_SECONDS")]
        since: Option<String>,
    },
    /// Prune old beats and compact the state database.
    Maintenance {
        /// Delete beats older than this many days (keeps the most recent per task).
        #[arg(long, value_name = "DAYS")]
        prune_beats: Option<i64>,
        /// Minimum most-recent beats kept per task when pruning.
        #[arg(long, value_name = "N", default_value_t = 20)]
        keep_per_task: i64,
    },
    /// Print compliance mapping to OWASP Agentic Top 10, NIST AI RMF, and EU AI Act.
    Compliance {
        /// Filter framework (owasp-agentic-top10, nist-ai-rmf, eu-ai-act, soc2).
        #[arg(long)]
        framework: Option<String>,
        /// Output format.
        #[arg(long, value_enum, default_value_t = Format::Text)]
        format: Format,
    },
    /// Recompute workflow event hash chain and report first divergence.
    AuditChain {
        /// Output format.
        #[arg(long, value_enum, default_value_t = Format::Text)]
        format: Format,
    },
    /// Generate shell completions.
    Completions {
        /// Target shell (bash, zsh, fish, powershell, elvish).
        #[arg(value_enum)]
        shell: clap_complete::Shell,
    },
    /// Generate man page documentation.
    Man {
        /// Output directory for man pages.
        #[arg(value_hint = ValueHint::DirPath, value_name = "DIR")]
        dir: PathBuf,
    },
}

#[cfg(test)]
#[path = "cli_tests.rs"]
mod cli_tests;
