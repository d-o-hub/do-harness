//! Unified CLI for the do-harness agent execution harness.
//!
//! Entrypoints: `verify` (computational sensors), `list` (sensor names),
//! `init-db` (migrations), `seed` (architecture invariants from
//! `plans/invariants.json`), `init` (workspace scaffold), `task` (task
//! state), `trace` (interaction traces), `distill` (heuristic extraction),
//! `eval` (skill-eval runner), `hook` (git hook management), and `version`
//! (version information).

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{ArgAction, Parser, Subcommand};

use crate::commands::{CliError, run_verify};
use crate::report::Format;

mod audit_chain;
mod commands;
mod config;
mod dbcheck;
mod distill;
mod doctor;
mod errors;
mod eval;
mod eval_assert;
mod eval_integrity;
mod eval_sandbox;
mod eval_walk;
mod evidence;
mod hook_script;
mod hooks;
mod init;
mod methods;
mod metrics;
mod report;
mod sensors;
mod skill_write;
mod task;
mod telemetry;
mod trace;
mod version;

/// Unified entrypoint for harness sensors and database maintenance.
#[derive(Debug, Parser)]
#[command(
    name = "do-harness",
    about = "do-harness agent execution harness CLI",
    version = version::version_str()
)]
struct Cli {
    /// Workspace root override (default: walk up from cwd).
    #[arg(long, global = true)]
    root: Option<PathBuf>,
    /// Explicit path to do-harness.toml.
    #[arg(long, global = true)]
    config: Option<PathBuf>,
    #[command(subcommand)]
    command: Command,
}

/// Available subcommands.
#[derive(Debug, Subcommand)]
enum Command {
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
        /// Write a machine-readable evidence artifact to path.
        #[arg(long)]
        evidence: Option<PathBuf>,
        /// Enforce strong evidence: exit non-zero on skips or missing timing/exit codes.
        #[arg(long)]
        strict: bool,
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
    /// Audit the workflow event log hash chain for tampering.
    AuditChain,
    /// Report harness trends: sensor stats, strikes, eval pass-rate history.
    Metrics {
        /// Output format.
        #[arg(long, value_enum, default_value_t = Format::Text)]
        format: Format,
    },
}

/// Available task-state actions.
#[derive(Debug, Subcommand)]
enum TaskAction {
    /// Write the task list to plans/tasks.json.
    Export,
    /// Print tasks from the state database.
    List {
        /// Output format.
        #[arg(long, value_enum, default_value_t = Format::Text)]
        format: Format,
    },
    /// Add a task in `pending` state.
    Add {
        /// Human-readable task title.
        title: String,
        /// Name of the HTN method this task follows.
        #[arg(long)]
        method: Option<String>,
        /// Parent task id.
        #[arg(long)]
        parent: Option<i64>,
        /// Recorded precondition guard.
        #[arg(long)]
        precondition: Option<String>,
    },
    /// Advance the task's subtask pointer.
    Advance {
        /// Task id.
        id: i64,
    },
    /// Mark a task done once its sensor-gated subtasks have passed.
    Done {
        /// Task id.
        id: i64,
    },
    /// Mark a task failed.
    Fail {
        /// Task id.
        id: i64,
    },
}

/// Available error-signature actions.
#[derive(Debug, Subcommand)]
enum ErrorsAction {
    /// List fail-fast error signatures.
    List {
        /// Scope to one task id.
        #[arg(long)]
        task: Option<i64>,
        /// Output format.
        #[arg(long, value_enum, default_value_t = Format::Text)]
        format: Format,
    },
    /// Clear fail-fast error signatures (optionally for one sensor/task).
    Clear {
        /// Only clear this signature key.
        #[arg(long)]
        sensor: Option<String>,
        /// Only clear signatures for this task id.
        #[arg(long)]
        task: Option<i64>,
    },
}

/// Available trace actions.
#[derive(Debug, Subcommand)]
enum TraceAction {
    /// Record a trace of an executed command and its resolution.
    Add {
        /// Session identifier grouping related traces.
        #[arg(long)]
        session: String,
        /// Owning task id.
        #[arg(long)]
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
}

/// Available hook-management actions.
#[derive(Debug, Copy, Clone, Subcommand)]
enum HookAction {
    /// Write pre-commit and pre-push hooks into `.git/hooks/`.
    Install {
        /// Overwrite foreign (unmanaged) hook files.
        #[arg(long)]
        force: bool,
    },
    /// Remove managed hooks, leaving foreign hook files untouched.
    Uninstall,
    /// Show whether the managed hooks and release binary are present.
    Status,
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("error: {err:#}");
            ExitCode::from(err.exit_code())
        }
    }
}

/// Dispatches the parsed CLI and classifies failures.
#[allow(clippy::too_many_lines)]
async fn run(cli: Cli) -> std::result::Result<(), CliError> {
    if let Command::Version { format } = cli.command {
        commands::print_version(format);
        return Ok(());
    }

    let root = match &cli.command {
        Command::Init { .. } => {
            commands::init_target(cli.root.as_deref()).map_err(CliError::Usage)?
        }
        _ => commands::resolve_root(cli.root.as_deref()).map_err(CliError::Usage)?,
    };
    match cli.command {
        Command::Version { .. } => unreachable!(),
        Command::Init { language, force } => {
            let opts = init::InitOpts { language, force };
            let report = init::init_workspace(&root, &opts)
                .await
                .map_err(CliError::Usage)?;
            commands::print_init(&report, &root, language);
            Ok(())
        }
        Command::Verify {
            fail_fast,
            format,
            only,
            record,
            task,
            evidence,
            strict,
        } => {
            run_verify(
                &root,
                cli.config.as_deref(),
                fail_fast,
                format,
                only,
                record,
                task,
                evidence,
                strict,
            )
            .await
        }
        Command::List { format } => {
            let cfg = config::load(&root, cli.config.as_deref()).map_err(CliError::Usage)?;
            report::print_names(&cfg.sensor_names(), format);
            Ok(())
        }
        Command::InitDb => commands::init_db(&root).await.map_err(CliError::Usage),
        Command::Seed => commands::seed(&root).await.map_err(CliError::Usage),
        Command::Task { action } => commands::task_cmd(&root, action)
            .await
            .map_err(CliError::Usage),
        Command::Trace { action } => commands::trace_cmd(&root, action)
            .await
            .map_err(CliError::Usage),
        Command::Distill {
            skill,
            pattern,
            description,
            from_trace,
            to_fixture,
        } => distill::distill(
            &root,
            &skill,
            &pattern,
            description.as_deref(),
            from_trace,
            to_fixture,
        )
        .await
        .map_err(CliError::Usage),
        Command::Errors { action } => commands::errors_cmd(&root, action)
            .await
            .map_err(CliError::Usage),
        Command::Eval { skill, bless } => eval::run_eval(&root, skill.as_deref(), bless)
            .await
            .map_err(CliError::Verify),
        Command::Hook { action } => {
            commands::hook(&root, cli.config.as_deref(), action).map_err(CliError::Usage)
        }
        Command::Doctor => doctor::run(&root).await.map_err(CliError::Verify),
        Command::AuditChain => {
            let report = audit_chain::audit_chain(&root)
                .await
                .map_err(CliError::Usage)?;
            if report.intact {
                println!("chain intact ({} event(s) verified)", report.count);
                Ok(())
            } else {
                let seq = report.tampered_seq.unwrap_or(0);
                eprintln!("chain tampered at sequence {seq}");
                Err(CliError::Verify(anyhow::anyhow!(
                    "chain tampered at sequence {seq}"
                )))
            }
        }
        Command::Metrics { format } => metrics::run_metrics(&root, format)
            .await
            .map_err(CliError::Usage),
    }
}
