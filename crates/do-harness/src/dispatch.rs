//! CLI dispatch: resolve the workspace root and route each subcommand.
//!
//! Extracted from `main.rs` so the binary's entrypoint stays a thin shell and
//! the dispatch table is not a second, unrelated responsibility on the same
//! file.

use anyhow::Context;
use std::path::{Path, PathBuf};

use clap::CommandFactory;

use crate::cli::{Cli, Command, DoctorAction};
use crate::{
    CliError, ci_explain, commands, config, distill, distill_strikes, doctor, dora, eval, exit,
    explain, feedback, init, learn, loc, man, metrics, overlap, pr, report, skills, split, status,
    telemetry,
};

/// Executes the parsed CLI, classifying every failure into an exit code.
pub async fn run(cli: Cli) -> Result<(), CliError> {
    // Commands below never touch a workspace root: they print metadata or
    // generate assets, so they must work outside a configured repository.
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
            man::render_page(dir)?;
            return Ok(());
        }
        _ => {}
    }

    let root = resolve_root(&cli)?;
    do_harness_db::root::validate_state_paths(&root).map_err(|e| CliError::Usage(e.into()))?;

    // An explicitly targeted directory selects exactly `<root>/do-harness.toml`
    // as a required explicit file: it must not silently fall back to the
    // built-in pack, nor to a policy discovered from the invocation cwd. With
    // neither override, implicit discovery and the built-in default remain.
    let config_path = cli
        .config
        .clone()
        .or_else(|| cli.root.as_ref().map(|_| root.join("do-harness.toml")));

    route(&root, config_path, cli).await
}

/// Resolves the workspace root for the subcommand that was asked to run.
fn resolve_root(cli: &Cli) -> Result<PathBuf, CliError> {
    let resolved = match &cli.command {
        Command::Init { .. } => commands::init_target(cli.root.as_deref()).map_err(CliError::Usage),
        Command::Pr { .. } | Command::CiExplain { .. } | Command::Sync { .. } => {
            pr::command::resolve_root(cli.root.as_deref()).map_err(CliError::Usage)
        }
        _ => commands::resolve_root(cli.root.as_deref()).map_err(CliError::Usage),
    }?;
    resolved
        .canonicalize()
        .map(crate::util::without_verbatim_prefix)
        .with_context(|| format!("failed to resolve workspace root {}", resolved.display()))
        .map_err(CliError::Usage)
}

/// Routes a root-bound subcommand.
#[allow(clippy::too_many_lines)]
async fn route(root: &Path, config_path: Option<PathBuf>, cli: Cli) -> Result<(), CliError> {
    match cli.command {
        Command::Version { .. }
        | Command::Compliance { .. }
        | Command::Completions { .. }
        | Command::Man { .. } => unreachable!("version/compliance/completions/man handled above"),
        Command::Pr { action } => pr::command::run(root, action, cli.dry_run),
        Command::Sync {
            switch,
            read_only,
            reset_diverged,
            status,
            format,
        } => {
            crate::sync::run(
                root,
                config_path.as_deref(),
                crate::sync::SyncOpts {
                    switch,
                    read_only,
                    reset_diverged,
                    status,
                },
                cli.dry_run,
                format,
            )
            .await
        }
        Command::Job { action } => match action {
            crate::cli::JobAction::Run {
                spec,
                resume,
                format,
            } => crate::job::run(root, &spec, resume, cli.dry_run, format)
                .await
                .map_err(exit::classify),
            crate::cli::JobAction::Status { id, format } => {
                crate::job::status(root, id.as_deref(), format).map_err(exit::classify)
            }
        },
        Command::Maintenance {
            prune_beats,
            keep_per_task,
        } => commands::maintenance(root, prune_beats, keep_per_task, cli.dry_run)
            .await
            .map_err(CliError::Usage),
        Command::Cleanup(args) => crate::cleanup::run(
            root,
            &crate::cleanup::Options {
                dry_run: cli.dry_run,
                format: args.format,
                keep: args.keep,
                worktrees: args.worktree,
                target_dirs: args.target_dir,
                include_shared: args.include_shared,
                optimistic: args.optimistic,
            },
        )
        .map_err(exit::classify),
        Command::Init {
            language,
            force,
            format,
            no_seed,
            minimal,
            no_gitignore,
            inspect,
        } => {
            if inspect {
                init::run_inspect(root, language, format).map_err(CliError::Usage)?;
                return Ok(());
            }
            let opts = init::InitOpts {
                language,
                force,
                no_seed,
                minimal,
                no_gitignore,
            };
            let mut report = init::init_workspace(root, &opts)
                .await
                .map_err(CliError::Usage)?;
            init::run_baseline(root, &mut report)
                .await
                .map_err(CliError::Usage)?;
            init::print_report(&report, root, format);
            if report
                .baseline
                .as_ref()
                .is_some_and(|baseline| baseline.state == init::BaselineState::Red)
            {
                Err(CliError::Verify(anyhow::anyhow!(
                    "initial verification is RED; fix the failures above and re-run init or verify"
                )))
            } else {
                Ok(())
            }
        }
        Command::Setup { action } => match action {
            crate::cli::SetupAction::Web { yes } => {
                crate::setup::web::run(root, yes, cli.dry_run).map_err(CliError::Usage)
            }
        },
        verify_command @ Command::Verify { .. } => {
            crate::verify_dispatch::run(
                root,
                config_path,
                verify_command,
                cli.dry_run,
                cli.quiet,
                cli.output,
                cli.config.is_some(),
            )
            .await
        }
        Command::List(args) => {
            let (sets, format) = (args.sets, args.format);
            let cfg = config::load(root, config_path.as_deref())
                .await
                .map_err(CliError::Usage)?;
            if sets {
                report::print_names(&crate::signals::available(&cfg), format)
                    .map_err(CliError::Usage)?;
            } else {
                report::print_names(&cfg.sensor_names(), format).map_err(CliError::Usage)?;
            }
            Ok(())
        }
        Command::Explain {
            set,
            changed,
            since,
            format,
        } => explain::run(root, config_path.as_deref(), set, changed, since, format).await,
        Command::Status {
            set,
            evidence,
            since,
            format,
        } => status::run(root, config_path.as_deref(), set, evidence, format, since).await,
        Command::Loc {
            dir: dir_args,
            ext: ext_args,
            format,
            warn,
            paths,
        } => loc::run(
            root,
            config_path.as_deref(),
            format,
            warn,
            &dir_args,
            &ext_args,
            &paths,
        )
        .map_err(CliError::Usage),
        Command::Split {
            file,
            dry_run,
            target,
        } => split::run(root, &file, dry_run, target.as_deref()).map_err(CliError::Usage),
        Command::InitDb {
            check,
            dry_run,
            yes,
        } => commands::init_db(
            root,
            &commands::InitDbOpts {
                check,
                dry_run,
                yes,
            },
        )
        .await
        .map_err(CliError::Usage),
        Command::Seed { prune } => commands::seed(root, prune, cli.dry_run)
            .await
            .map_err(CliError::Usage),
        Command::Task { action } => commands::task_cmd(root, action, cli.dry_run)
            .await
            .map_err(CliError::Usage),
        Command::Trace { action } => commands::trace_cmd(root, action)
            .await
            .map_err(CliError::Usage),
        Command::Distill(args) => {
            if args.from_strikes {
                distill_strikes::scaffold_from_strikes(
                    root,
                    args.skill.as_deref(),
                    args.task,
                    args.min_strikes.unwrap_or(telemetry::FAIL_FAST_STRIKES),
                    args.dry_run,
                    args.format,
                )
                .await
                .map_err(CliError::Usage)?;
                return Ok(());
            }
            let Some(skill) = args.skill else {
                return Err(CliError::Usage(anyhow::anyhow!(
                    "distill requires --skill (or --from-strikes to scaffold from recorded strikes)"
                )));
            };
            let Some(pattern) = args.pattern else {
                return Err(CliError::Usage(anyhow::anyhow!(
                    "distill requires --pattern describing the generalized fix"
                )));
            };
            let Some(from_trace) = args.from_trace else {
                return Err(CliError::Usage(anyhow::anyhow!(
                    "distill requires evidence: pass --from-trace <id> of a resolved trace (see do-harness trace add)"
                )));
            };
            distill::distill(
                root,
                &skill,
                &pattern,
                args.description.as_deref(),
                Some(from_trace),
                args.to_fixture,
                args.dry_run,
                args.format,
            )
            .await
            .map_err(CliError::Usage)
        }
        Command::Errors { action } => commands::errors_cmd(root, action)
            .await
            .map_err(CliError::Usage),
        Command::Eval {
            skill,
            bless,
            list_skills,
            fail_fast,
            dry_run,
            format,
            approver,
            no_lift,
            agent_cmd,
            agent_timeout,
            strict_fixtures,
            trials,
        } => eval::run_eval(
            root,
            eval::EvalOpts {
                skill: skill.as_deref(),
                bless,
                list_skills,
                fail_fast,
                dry_run,
                format,
                approver: approver.as_deref(),
                no_lift,
                agent_cmd: agent_cmd.as_deref(),
                agent_timeout_secs: agent_timeout,
                strict_fixtures,
                trials,
            },
        )
        .await
        .map_err(exit::classify),
        Command::Hook { action } => {
            commands::hook(root, config_path.as_deref(), action, cli.dry_run)
                .await
                .map_err(CliError::Usage)
        }
        Command::Doctor {
            action,
            format,
            strict,
            client_profile,
            client_probe,
        } => match action {
            Some(DoctorAction::PrPolicy { format: sub_format }) => {
                pr::policy::run(root, sub_format.unwrap_or(format))
            }
            None => doctor::run(
                root,
                format,
                strict,
                client_profile.as_deref(),
                client_probe,
            )
            .await
            .map_err(exit::classify),
        },
        Command::AuditChain { format } => commands::audit_chain_cmd(root, format)
            .await
            .map_err(CliError::Verify),
        Command::Dora {
            days,
            format,
            record,
            source,
            now,
        } => dora::command::run(root, days, format, record, source, now).await,
        Command::CiExplain { run_id, format } => ci_explain::run(root, &run_id, format).await,
        Command::Learn { args } => {
            if !args.draft {
                return Err(CliError::Usage(anyhow::anyhow!(
                    "learn requires --draft: it only proposes changes, never applies them"
                )));
            }
            let () = learn::run(root, Some(args.days), Some(args.min_fires), args.format)
                .await
                .map_err(CliError::Usage)?;
            Ok(())
        }
        Command::Metrics {
            action,
            format,
            sensor,
            skill,
            since,
            scope,
            task,
            branch,
            all,
        } => {
            let filter = metrics::MetricsFilter {
                sensor: sensor.as_deref(),
                skill: skill.as_deref(),
                since: since.as_deref(),
                until: None,
                scope: scope.as_deref(),
                task,
                branch: branch.as_deref(),
                all,
            };
            metrics::dispatch(root, action, format, filter).await
        }
        Command::Migrate(args) => {
            crate::migrate::run(
                root,
                config_path.as_deref(),
                args.diff,
                args.write,
                args.format,
            )
            .await
        }
        Command::Report(args) => {
            let filter = metrics::MetricsFilter {
                sensor: args.sensor.as_deref(),
                skill: None,
                since: args.since.as_deref(),
                until: args.until.as_deref(),
                scope: args.scope.as_deref(),
                task: args.task,
                branch: args.branch.as_deref(),
                all: args.all,
            };
            crate::reports::run(root, &filter, args.format).await
        }
        Command::Sensor(args) => match args.action {
            crate::cli::SensorAction::New(args) => {
                crate::sensor_new::run(root, config_path.as_deref(), &args, cli.dry_run).await
            }
        },
        Command::Lint(args) => crate::lint::run(root, args.format).await,
        Command::Overlap { threshold, format } => {
            overlap::run_overlap(root, threshold, format).map_err(CliError::Usage)
        }
        Command::Skills { action } => skills::run(root, action),
        Command::Feedback { action } => feedback::run(root, action, cli.dry_run)
            .await
            .map_err(exit::classify),
    }
}
