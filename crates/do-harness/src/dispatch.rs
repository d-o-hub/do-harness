//! CLI dispatch: resolve the workspace root and route each subcommand.
//!
//! Extracted from `main.rs` so the binary's entrypoint stays a thin shell and
//! the dispatch table is not a second, unrelated responsibility on the same
//! file.

use std::path::{Path, PathBuf};

use clap::CommandFactory;

use crate::cli::{Cli, Command};
use crate::{
    CliError, ci_explain, commands, config, distill, distill_strikes, doctor, dora, eval, exit,
    explain, feedback, init, learn, loc, man, metrics, overlap, pr, report, sensors, skills, split,
    status, telemetry, verify,
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
    match &cli.command {
        Command::Init { .. } => commands::init_target(cli.root.as_deref()).map_err(CliError::Usage),
        Command::Pr { .. } | Command::CiExplain { .. } => {
            pr::command::resolve_root(cli.root.as_deref()).map_err(CliError::Usage)
        }
        _ => commands::resolve_root(cli.root.as_deref()).map_err(CliError::Usage),
    }
}

/// Routes a root-bound subcommand.
#[allow(clippy::too_many_lines)]
async fn route(root: &Path, config_path: Option<PathBuf>, cli: Cli) -> Result<(), CliError> {
    match cli.command {
        Command::Version { .. }
        | Command::Compliance { .. }
        | Command::Completions { .. }
        | Command::Man { .. } => unreachable!("version/compliance/completions/man handled above"),
        Command::Pr { action } => pr::command::run(root, action),
        Command::Maintenance {
            prune_beats,
            keep_per_task,
        } => commands::maintenance(root, prune_beats, keep_per_task, cli.dry_run)
            .await
            .map_err(CliError::Usage),
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
        Command::Verify {
            fail_fast,
            format,
            set,
            changed,
            only,
            exclude,
            jobs,
            unchanged,
            record,
            task,
            global,
            evidence,
            strict,
            bless,
            approver,
        } => {
            let (task_id, raw_task) = match task.as_deref() {
                Some("global") => (None, Some("global".to_string())),
                Some(raw) => {
                    let id = raw.parse::<i64>().map_err(|_| {
                        CliError::Usage(anyhow::anyhow!(
                            "invalid --task '{raw}': expected an integer task ID or 'global'"
                        ))
                    })?;
                    (Some(id), Some(raw.to_string()))
                }
                None => (None, None),
            };
            // `--task global` names the namespace just as explicitly as
            // `--global`, so it must not trigger the unscoped-record advisory.
            let explicit_global = global || raw_task.as_deref() == Some("global");
            let opts = sensors::VerifyOpts {
                fail_fast,
                set,
                changed,
                only,
                exclude,
                jobs,
                unchanged,
                record,
                task: task_id,
                raw_task,
                global: explicit_global,
                evidence,
                strict,
                bless,
                approver,
                format,
                quiet: cli.quiet,
                output: cli.output.clone(),
                config: config_path,
                ..Default::default()
            };
            verify::run(root, opts).await
        }
        Command::List { sets, format } => {
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
            format,
        } => explain::run(root, config_path.as_deref(), set, changed, format).await,
        Command::Status {
            set,
            evidence,
            format,
        } => status::run(root, config_path.as_deref(), set, evidence, format).await,
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
        Command::Distill {
            skill,
            pattern,
            description,
            from_trace,
            from_strikes,
            min_strikes,
            task,
            to_fixture,
            dry_run,
            format,
        } => {
            if from_strikes {
                distill_strikes::scaffold_from_strikes(
                    root,
                    skill.as_deref(),
                    task,
                    min_strikes.unwrap_or(telemetry::FAIL_FAST_STRIKES),
                    dry_run,
                    format,
                )
                .await
                .map_err(CliError::Usage)?;
                return Ok(());
            }
            let Some(skill) = skill else {
                return Err(CliError::Usage(anyhow::anyhow!(
                    "distill requires --skill (or --from-strikes to scaffold from recorded strikes)"
                )));
            };
            let Some(pattern) = pattern else {
                return Err(CliError::Usage(anyhow::anyhow!(
                    "distill requires --pattern describing the generalized fix"
                )));
            };
            let Some(from_trace) = from_trace else {
                return Err(CliError::Usage(anyhow::anyhow!(
                    "distill requires evidence: pass --from-trace <id> of a resolved trace (see do-harness trace add)"
                )));
            };
            distill::distill(
                root,
                &skill,
                &pattern,
                description.as_deref(),
                Some(from_trace),
                to_fixture,
                dry_run,
                format,
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
        Command::Doctor { format, strict } => doctor::run(root, format, strict)
            .await
            .map_err(exit::classify),
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
                scope: scope.as_deref(),
                task,
                branch: branch.as_deref(),
                all,
            };
            metrics::dispatch(root, action, format, filter).await
        }
        Command::Overlap { threshold, format } => {
            overlap::run_overlap(root, threshold, format).map_err(CliError::Usage)
        }
        Command::Skills { action } => skills::run(root, action),
        Command::Feedback { action } => feedback::run(root, action).await.map_err(exit::classify),
    }
}
