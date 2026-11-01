//! CLI argument definitions for the do-harness harness.
//!
//! Extracted from `main.rs` to keep that file under the 500 LOC ceiling.

use std::path::PathBuf;

use clap::{Parser, ValueHint};

mod actions;
mod command;
mod learn;
pub use actions::{
    ErrorsAction, FeedbackAction, HookAction, MetricsAction, PrAction, SkillsAction, TaskAction,
    TraceAction,
};
pub use command::Command;
pub use learn::LearnArgs;

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
    /// Suppress PASS/WARN verdict lines and the success footer.
    #[arg(short, long, global = true)]
    pub quiet: bool,
    /// Default evidence artifact path for verify (overrides the --set default;
    /// an explicit --evidence still wins).
    #[arg(long, global = true, value_hint = ValueHint::FilePath, value_name = "FILE")]
    pub output: Option<PathBuf>,
    /// Dry run without side effects.
    #[arg(long, global = true)]
    pub dry_run: bool,

    #[command(subcommand)]
    pub command: Command,
}

#[cfg(test)]
#[path = "cli_tests.rs"]
mod cli_tests;
