//! Bounded verification across independently configured workspaces.

use std::{path::PathBuf, sync::Arc};

use anyhow::{Context, anyhow};
use serde_json::Value;
use tokio::{
    process::Command,
    sync::Semaphore,
    task::{JoinSet, spawn_blocking},
};

use crate::{
    CliError, config,
    report::VerifyFormat,
    sensors::VerifyOpts,
    workspace_discovery::{
        discover_workspaces, is_workspace_relative, relative_path, workspace_change_scope,
    },
    workspace_report::{
        GlobalSelection, WorkspaceReport, WorkspaceResult, aggregate_exit_code, append_error,
        print_text,
    },
};

const REPORT_SCHEMA_VERSION: u32 = 1;

#[derive(Debug)]
struct Workspace {
    root: PathBuf,
    affected: bool,
    run_unfiltered: bool,
}

pub(crate) async fn run(
    root: &std::path::Path,
    pattern: &str,
    mut opts: VerifyOpts,
    explicit_config: bool,
) -> Result<(), CliError> {
    validate_options(root, &opts, explicit_config).await?;
    let (workspaces, selection) = discover_workspace_scope(root, pattern, &opts).await?;
    opts.config = None;
    let (results, jobs) = run_selected_workspaces(workspaces, &opts).await?;
    let exit_code = aggregate_exit_code(&results);
    let report = WorkspaceReport {
        schema_version: REPORT_SCHEMA_VERSION,
        root: root.to_string_lossy().into_owned(),
        pattern: pattern.to_owned(),
        mode: if opts.simulate {
            "simulate"
        } else if opts.dry_run {
            "dry-run"
        } else {
            "verify"
        },
        jobs,
        ok: exit_code == 0,
        exit_code,
        selection,
        workspaces: results,
    };

    match opts.format {
        VerifyFormat::Json => println!(
            "{}",
            serde_json::to_string(&report).map_err(|error| CliError::Usage(error.into()))?
        ),
        VerifyFormat::Text => print_text(&report, opts.quiet),
        VerifyFormat::Junit => unreachable!("Junit rejected before workspace execution"),
    }

    match exit_code {
        0 => Ok(()),
        1 => Err(CliError::Verify(anyhow!(
            "one or more workspaces failed verification"
        ))),
        2 => Err(CliError::Usage(anyhow!(
            "one or more workspaces could not be judged"
        ))),
        _ => Err(CliError::Running(anyhow!(
            "one or more workspace runs are still running or could not be judged"
        ))),
    }
}

async fn validate_options(
    root: &std::path::Path,
    opts: &VerifyOpts,
    explicit_config: bool,
) -> Result<(), CliError> {
    if explicit_config {
        return Err(CliError::Usage(anyhow!(
            "--config cannot be combined with --workspaces; each workspace loads its own do-harness.toml"
        )));
    }
    if opts.format == VerifyFormat::Junit {
        return Err(CliError::Usage(anyhow!(
            "--format junit is not supported with --workspaces; use text or json"
        )));
    }
    if opts.jobs == Some(0) {
        return Err(CliError::Usage(anyhow!("--jobs must be greater than zero")));
    }
    for (flag, path) in [
        ("--output", opts.output.as_deref()),
        ("--evidence", opts.evidence.as_deref()),
    ] {
        if path.is_some_and(|path| !is_workspace_relative(path)) {
            return Err(CliError::Usage(anyhow!(
                "{flag} must be a relative path inside each workspace when --workspaces is used"
            )));
        }
    }

    config::load(root, opts.config.as_deref())
        .await
        .map_err(CliError::Usage)?;
    Ok(())
}

async fn discover_workspace_scope(
    root: &std::path::Path,
    pattern: &str,
    opts: &VerifyOpts,
) -> Result<(Vec<Workspace>, Option<GlobalSelection>), CliError> {
    let root_for_discovery = root.to_path_buf();
    let pattern_for_discovery = pattern.to_owned();
    let roots =
        spawn_blocking(move || discover_workspaces(&root_for_discovery, &pattern_for_discovery))
            .await
            .context("workspace discovery task failed")
            .map_err(CliError::Usage)?
            .map_err(CliError::Usage)?;
    let global_changes = if opts.changed || opts.since.is_some() {
        let changed = opts.changed;
        let since = opts.since.clone();
        let root_for_changes = root.to_path_buf();
        Some(
            spawn_blocking(move || {
                crate::changes::discover_selection(&root_for_changes, changed, since.as_deref())
            })
            .await
            .context("global change discovery task failed")
            .map_err(CliError::Usage)?,
        )
    } else {
        None
    };

    if let Some(changes) = &global_changes {
        if !changes.changed.discovery_failed && changes.changed.files.is_empty() {
            return Err(CliError::Usage(anyhow!(
                "no change input: pass --since <REF> for committed changes or run verify without change filtering"
            )));
        }
    }

    let selection = global_changes.as_ref().map(|changes| GlobalSelection {
        mode: changes.mode.as_str().to_owned(),
        since: opts.since.clone(),
        changed_files: changes
            .changed
            .files
            .iter()
            .map(|file| file.path.clone())
            .collect(),
        discovery_error: changes.changed.discovery_error.clone(),
    });
    let workspaces = roots
        .into_iter()
        .map(|workspace_root| {
            let relative = relative_path(root, &workspace_root).map_err(CliError::Usage)?;
            let (affected, run_unfiltered) = match &selection {
                None => (true, false),
                Some(selection) if selection.discovery_error.is_some() => (true, false),
                Some(selection) => workspace_change_scope(&relative, &selection.changed_files),
            };
            Ok(Workspace {
                root: workspace_root,
                affected,
                run_unfiltered,
            })
        })
        .collect::<Result<Vec<_>, CliError>>()?;

    if selection.is_some() && workspaces.iter().all(|workspace| !workspace.affected) {
        return Err(CliError::Usage(anyhow!(
            "the global change set contains no paths under workspaces matching '{pattern}'"
        )));
    }

    Ok((workspaces, selection))
}

async fn run_selected_workspaces(
    workspaces: Vec<Workspace>,
    opts: &VerifyOpts,
) -> Result<(Vec<WorkspaceResult>, usize), CliError> {
    let requested_jobs = opts.jobs;
    let affected_count = workspaces
        .iter()
        .filter(|workspace| workspace.affected)
        .count();
    let concurrency = requested_jobs.unwrap_or(1).min(affected_count);
    let semaphore = Arc::new(Semaphore::new(concurrency));
    // Re-invoking our own binary for child workspace verifies is the intended
    // use (same pattern as eval_walk), not a security decision.
    // nosemgrep: rust.lang.security.current-exe.current-exe
    let executable = std::env::current_exe()
        .context("failed to locate do-harness executable")
        .map_err(CliError::Usage)?;
    let shared_opts = Arc::new(opts.clone());
    let mut results: Vec<Option<WorkspaceResult>> = std::iter::repeat_with(|| None)
        .take(workspaces.len())
        .collect();
    let mut tasks = JoinSet::new();

    for (index, workspace) in workspaces.iter().enumerate() {
        if !workspace.affected {
            results[index] = Some(WorkspaceResult {
                root: workspace.root.to_string_lossy().into_owned(),
                status: "skipped",
                ok: true,
                exit_code: 0,
                report: None,
                stderr: String::new(),
                reason: Some("no global changed paths matched this workspace".to_owned()),
            });
            continue;
        }
        let permit = Arc::clone(&semaphore)
            .acquire_owned()
            .await
            .map_err(|error| CliError::Running(anyhow!("workspace semaphore closed: {error}")))?;
        let executable = executable.clone();
        let root = workspace.root.clone();
        let run_unfiltered = workspace.run_unfiltered;
        let opts = Arc::clone(&shared_opts);
        let limit_sensor_jobs = requested_jobs.is_some();
        tasks.spawn(async move {
            let _permit = permit;
            (
                index,
                run_workspace(&executable, root, &opts, run_unfiltered, limit_sensor_jobs).await,
            )
        });
    }

    while let Some(result) = tasks.join_next().await {
        let (index, workspace_result) = result.map_err(|error| {
            CliError::Running(anyhow!("workspace verification task failed: {error}"))
        })?;
        results[index] = Some(workspace_result);
    }
    let results = results
        .into_iter()
        .enumerate()
        .map(|(index, result)| {
            result.ok_or_else(|| {
                CliError::Running(anyhow!("workspace result {index} was not produced"))
            })
        })
        .collect::<Result<Vec<_>, _>>()?;

    Ok((results, concurrency))
}

async fn run_workspace(
    executable: &std::path::Path,
    root: PathBuf,
    opts: &VerifyOpts,
    run_unfiltered: bool,
    limit_sensor_jobs: bool,
) -> WorkspaceResult {
    let mut command = Command::new(executable);
    command
        .arg("--root")
        .arg(&root)
        .arg("verify")
        .arg("--format")
        .arg("json")
        .kill_on_drop(true);
    append_verify_args(&mut command, opts, run_unfiltered, limit_sensor_jobs);
    let output = match command.output().await {
        Ok(output) => output,
        Err(error) => {
            return WorkspaceResult {
                root: root.to_string_lossy().into_owned(),
                status: "unjudged",
                ok: false,
                exit_code: 2,
                report: None,
                stderr: format!("failed to launch workspace verifier: {error}"),
                reason: Some("workspace verifier could not be started".to_owned()),
            };
        }
    };

    let mut stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    let report = match serde_json::from_slice::<Value>(&output.stdout) {
        Ok(value) if value.is_object() => Some(value),
        Ok(_) => {
            append_error(
                &mut stderr,
                "workspace verifier returned a non-object JSON report",
            );
            None
        }
        Err(error) => {
            append_error(
                &mut stderr,
                &format!("invalid workspace JSON report: {error}"),
            );
            None
        }
    };
    let mut exit_code = output
        .status
        .code()
        .filter(|code| (0..=3).contains(code))
        .unwrap_or(3);
    if exit_code == 0 && report.is_none() {
        exit_code = 3;
    }
    if !opts.simulate {
        if let Some(report) = &report {
            match report.get("ok").and_then(Value::as_bool) {
                Some(true) if exit_code != 0 => exit_code = 3,
                Some(false) if exit_code == 0 => exit_code = 1,
                None => exit_code = 3,
                _ => {}
            }
        }
    }
    let status = if opts.simulate {
        "planned"
    } else {
        match exit_code {
            0 => "passed",
            1 => "failed",
            _ => "unjudged",
        }
    };
    WorkspaceResult {
        root: root.to_string_lossy().into_owned(),
        status,
        ok: exit_code == 0,
        exit_code,
        report,
        stderr,
        reason: None,
    }
}

fn append_verify_args(
    command: &mut Command,
    opts: &VerifyOpts,
    run_unfiltered: bool,
    limit_sensor_jobs: bool,
) {
    for (enabled, flag) in [
        (opts.fail_fast, "--fail-fast"),
        (opts.dry_run, "--dry-run"),
        (opts.simulate, "--simulate"),
        (opts.strict, "--strict"),
        (opts.record, "--record"),
        (opts.bless, "--bless"),
        (opts.global, "--global"),
        (opts.quiet, "--quiet"),
    ] {
        if enabled {
            command.arg(flag);
        }
    }
    if let Some(set) = &opts.set {
        command.arg("--set").arg(set);
    }
    if !run_unfiltered {
        if opts.changed {
            command.arg("--changed");
        }
        if let Some(since) = &opts.since {
            command.arg("--since").arg(since);
        }
    }
    for sensor in &opts.only {
        command.arg("--only").arg(sensor);
    }
    for sensor in &opts.exclude {
        command.arg("--exclude").arg(sensor);
    }
    if limit_sensor_jobs {
        command.arg("--jobs").arg("1");
    }
    let unchanged = match opts.unchanged {
        crate::sensors::UnchangedMode::Warn => "warn",
        crate::sensors::UnchangedMode::Skip => "skip",
        crate::sensors::UnchangedMode::Run => "run",
    };
    command.arg("--unchanged").arg(unchanged);
    if let Some(task) = &opts.raw_task {
        if task != "global" {
            command.arg("--task").arg(task);
        }
    }
    if let Some(evidence) = &opts.evidence {
        command.arg("--evidence").arg(evidence);
    }
    if let Some(approver) = &opts.approver {
        command.arg("--approver").arg(approver);
    }
    if let Some(output) = &opts.output {
        command.arg("--output").arg(output);
    }
}
