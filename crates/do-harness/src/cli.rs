//! CLI argument definitions for the do-harness harness.
//!
//! Extracted from `main.rs` to keep that file under the 500 LOC ceiling.

use std::path::PathBuf;

use clap::{ArgAction, Parser, Subcommand};

use crate::init;
use crate::report::Format;
use crate::{ErrorsAction, HookAction, TaskAction, TraceAction};

/// Unified entrypoint for harness sensors and database maintenance.
#[derive(Debug, Parser)]
#[command(
    name = "do-harness",
    about = "do-harness agent execution harness CLI",
    version = crate::version::version_str()
)]
pub struct Cli {
    /// Workspace root override (default: walk up from cwd).
    #[arg(long, global = true)]
    pub root: Option<PathBuf>,
    /// Explicit path to do-harness.toml.
    #[arg(long, global = true)]
    pub config: Option<PathBuf>,
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
    Verify {
        /// Halt at the first failing sensor.
        #[arg(long)]
        fail_fast: bool,
        /// Output format.
        #[arg(long, value_enum, default_value_t = Format::Text)]
        format: Format,
        /// Run only the named sensor (repeatable).
        #[arg(long = "only", action = ArgAction::Append)]
        only: Vec<String>,
        /// Persist beats and error signatures into the state database.
        #[arg(long)]
        record: bool,
        /// Scope records and fail-fast strikes to this task id.
        #[arg(long)]
        task: Option<i64>,
    },
    /// List sensor names.
    List {
        /// Output format.
        #[arg(long, value_enum, default_value_t = Format::Text)]
        format: Format,
    },
    /// Apply pending database migrations.
    InitDb,
    /// Seed invariants from plans/invariants.json.
    Seed,
    /// Scaffold a harness workspace in a target directory.
    Init {
        /// Language pack to scaffold.
        #[arg(long, value_enum, default_value_t = init::Language::Rust)]
        language: init::Language,
        /// Overwrite existing files.
        #[arg(long)]
        force: bool,
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
        #[arg(long)]
        skill: String,
        /// Generalized pattern to record.
        #[arg(long)]
        pattern: String,
        /// When the pattern applies.
        #[arg(long)]
        description: Option<String>,
        /// Source trace id; required as evidence of a resolved fix.
        #[arg(long = "from-trace")]
        from_trace: Option<i64>,
        /// Raise the skill's pass-rate bar after this recovery.
        #[arg(long = "to-fixture")]
        to_fixture: bool,
    },
    /// Inspect and clear fail-fast error signatures.
    Errors {
        #[command(subcommand)]
        action: ErrorsAction,
    },
    /// Validate skill structure and benchmark skill evals.
    Eval {
        /// Restrict evaluation to this skill directory name.
        #[arg(long)]
        skill: Option<String>,
        /// Bless a fully green run: re-baseline graders and raise the bar.
        #[arg(long)]
        bless: bool,
    },
    /// Manage git hooks that run `do-harness verify`.
    Hook {
        #[command(subcommand)]
        action: HookAction,
    },
    /// Run diagnostic checks on binary resolution and git hook health.
    Doctor,
    /// Report harness trends: sensor stats, strikes, eval pass-rate history.
    Metrics {
        /// Output format.
        #[arg(long, value_enum, default_value_t = Format::Text)]
        format: Format,
    },
    /// Print compliance mapping to OWASP Agentic Top 10, NIST AI RMF, and EU AI Act.
    Compliance {
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
}
