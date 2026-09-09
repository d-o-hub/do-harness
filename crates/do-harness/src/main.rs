//! Unified CLI for the do-harness agent execution harness.
//!
//! Entrypoints: `verify` (computational sensors), `list` (sensor names),
//! `init-db` (migrations), `seed` (architecture invariants from
//! `plans/invariants.json`), `init` (workspace scaffold), `task` (task
//! state), `trace` (interaction traces), `distill` (heuristic extraction),
//! `eval` (skill-eval runner), `hook` (git hook management), and `version`
//! (version information).

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{CommandFactory, Parser};

use crate::cli::{Cli, Command};
use crate::report::Format;

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
mod hook_script;
mod hooks;
mod init;
mod methods;
mod metrics;
mod policy;
mod report;
mod sensors;
mod skill_write;
mod task;
mod telemetry;
mod trace;
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
            man.render(&mut buffer).map_err(|e| CliError::Usage(e.into()))?;
            std::fs::write(dir.join("do-harness.1"), buffer).map_err(|e| CliError::Usage(e.into()))?;
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
        | Command::Man { .. } => unreachable!(),
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
            run_verify(
                &root,
                cli.config.as_deref(),
                fail_fast,
                format,
                only,
                exclude,
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
        Command::Hook { action } => {
            commands::hook(&root, cli.config.as_deref(), action).map_err(CliError::Usage)
        }
        Command::Doctor { format, strict } => {
            doctor::run(&root, format, strict).await.map_err(CliError::Verify)
        }
        Command::AuditChain { format } => commands::audit_chain_cmd(&root, format)
            .await
            .map_err(CliError::Verify),
        Command::Metrics { format, sensor, skill, since } => metrics::run_metrics(
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

/// Runs the `verify` subcommand: sensors, optional beat recording, report.
#[allow(clippy::too_many_arguments)]
async fn run_verify(
    root: &Path,
    config: Option<&Path>,
    fail_fast: bool,
    format: Format,
    only: Vec<String>,
    exclude: Vec<String>,
    record: bool,
    task: Option<i64>,
    evidence: Option<PathBuf>,
    strict: bool,
) -> std::result::Result<(), CliError> {
    let started_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX));
    let cfg = config::load(root, config).map_err(CliError::Usage)?;
    let blocked = if record {
        telemetry::blocked_sensors(root, &cfg.sensor_names(), task)
            .await
            .map_err(CliError::Usage)?
    } else {
        Vec::new()
    };
    let opts = sensors::VerifyOpts {
        fail_fast,
        only,
        exclude,
        blocked,
    };
    match sensors::verify(&cfg, root, &opts) {
        Ok(report) => {
            if record {
                telemetry::record_verify(root, &report, &opts.blocked, task)
                    .await
                    .map_err(CliError::Usage)?;
            }
            report::print_report(&report, format);

            let evidence_path = evidence
                .or_else(|| strict.then(|| PathBuf::from(".do-harness/evidence.json")))
                .map(|p| if p.is_relative() { root.join(p) } else { p });

            if let Some(path) = evidence_path {
                let finished_at = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX));
                let doc = evidence::EvidenceDocument::from_run(
                    &cfg,
                    root,
                    &report,
                    &opts.only,
                    task,
                    started_at,
                    finished_at,
                );
                if let Some(parent) = path.parent() {
                    if !parent.as_os_str().is_empty() {
                        let _ = std::fs::create_dir_all(parent);
                    }
                }
                let json =
                    serde_json::to_vec_pretty(&doc).map_err(|e| CliError::Verify(e.into()))?;
                std::fs::write(&path, json).map_err(|e| CliError::Verify(e.into()))?;

                if strict && !doc.is_strict_clean() {
                    eprintln!(
                        "strict evidence check failed; artifact at {}",
                        path.display()
                    );
                    return Err(CliError::Verify(anyhow::anyhow!("weak evidence")));
                }
            }

            if report.ok {
                Ok(())
            } else {
                Err(CliError::Verify(anyhow::anyhow!(
                    "{} sensor(s) failed",
                    report.failed.len()
                )))
            }
        }
        Err(err) => Err(CliError::Usage(err)),
    }
}
