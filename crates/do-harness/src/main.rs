//! Unified CLI for the do-harness agent execution harness.
//!
//! Entrypoints: `verify` (computational sensors), `list` (sensor names),
//! `init-db` (migrations), `seed` (architecture invariants from
//! `plans/invariants.json`), `init` (workspace scaffold), `task` (task
//! state), `trace` (interaction traces), `distill` (heuristic extraction),
//! `eval` (skill-eval runner), `hook` (git hook management), and `version`
//! (version information).

use std::process::ExitCode;

use clap::{CommandFactory, Parser};

use crate::cli::{Cli, Command};

mod audit;
mod cli;
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
mod fs_perm;
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
mod verify;
mod version;

/// Classified CLI failure carrying its process exit code.
pub enum CliError {
    /// Usage, config, or discovery problems: exit 2.
    Usage(anyhow::Error),
    /// Sensor verification failed: exit 1.
    Verify(anyhow::Error),
}

impl CliError {
    /// The process exit code for this error class.
    #[must_use]
    pub fn exit_code(&self) -> u8 {
        match self {
            CliError::Usage(_) => 2,
            CliError::Verify(_) => 1,
        }
    }
}

impl std::fmt::Display for CliError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CliError::Usage(err) | CliError::Verify(err) => write!(f, "{err:#}"),
        }
    }
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
    match &cli.command {
        Command::Version { format } => {
            commands::print_version(*format);
            return Ok(());
        }
        Command::Compliance { framework, format } => {
            commands::print_compliance_filtered(framework.as_deref(), *format);
            return Ok(());
        }
        Command::Completions { shell } => {
            let mut cmd = Cli::command();
            clap_complete::generate(*shell, &mut cmd, "do-harness", &mut std::io::stdout());
            return Ok(());
        }
        Command::Man { dir } => {
            let cmd = Cli::command();
            let man = clap_mangen::Man::new(cmd);
            std::fs::create_dir_all(dir).map_err(|e| CliError::Usage(e.into()))?;
            let mut buffer = Vec::new();
            man.render(&mut buffer)
                .map_err(|e| CliError::Usage(e.into()))?;
            std::fs::write(dir.join("do-harness.1"), buffer)
                .map_err(|e| CliError::Usage(e.into()))?;
            return Ok(());
        }
        _ => {}
    }

    let root = match &cli.command {
        Command::Init { .. } => {
            commands::init_target(cli.root.as_deref()).map_err(CliError::Usage)?
        }
        _ => commands::resolve_root(cli.root.as_deref()).map_err(CliError::Usage)?,
    };

    match cli.command {
        Command::Version { .. }
        | Command::Compliance { .. }
        | Command::Completions { .. }
        | Command::Man { .. } => unreachable!("version/compliance/completions/man handled above"),
        Command::Maintenance {
            prune_beats,
            keep_per_task,
        } => commands::maintenance(&root, prune_beats, keep_per_task)
            .await
            .map_err(CliError::Usage),
        Command::Init {
            language,
            force,
            format: _,
            no_seed: _,
            minimal: _,
            no_gitignore: _,
        } => {
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
            exclude,
            record,
            task,
            evidence,
            strict,
        } => {
            let opts = sensors::VerifyOpts {
                fail_fast,
                only,
                exclude,
                record,
                task,
                evidence,
                strict,
                format,
                config: cli.config.clone(),
                ..Default::default()
            };
            verify::run(&root, opts).await
        }
        Command::List { format } => {
            let cfg = config::load(&root, cli.config.as_deref())
                .await
                .map_err(CliError::Usage)?;
            report::print_names(&cfg.sensor_names(), format);
            Ok(())
        }
        Command::InitDb {
            check,
            dry_run,
            yes,
        } => commands::init_db(
            &root,
            &commands::InitDbOpts {
                check,
                dry_run,
                yes,
            },
        )
        .await
        .map_err(CliError::Usage),
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
            dry_run,
            format,
        } => distill::distill(
            &root,
            &skill,
            &pattern,
            description.as_deref(),
            from_trace,
            to_fixture,
            dry_run,
            format,
        )
        .await
        .map_err(CliError::Usage),
        Command::Errors { action } => commands::errors_cmd(&root, action)
            .await
            .map_err(CliError::Usage),
        Command::Eval {
            skill,
            bless,
            list_skills,
            fail_fast,
            dry_run,
            format,
        } => eval::run_eval(
            &root,
            skill.as_deref(),
            bless,
            list_skills,
            fail_fast,
            dry_run,
            format,
        )
        .await
        .map_err(CliError::Verify),
        Command::Hook { action } => commands::hook(&root, cli.config.as_deref(), action)
            .await
            .map_err(CliError::Usage),
        Command::Doctor { format, strict } => doctor::run(&root, format, strict)
            .await
            .map_err(CliError::Verify),
        Command::AuditChain { format } => commands::audit_chain_cmd(&root, format)
            .await
            .map_err(CliError::Verify),
        Command::Metrics {
            format,
            sensor,
            skill,
            since,
        } => metrics::run_metrics(
            &root,
            format,
            sensor.as_deref(),
            skill.as_deref(),
            since.as_deref(),
        )
        .await
        .map_err(CliError::Usage),
    }
}
