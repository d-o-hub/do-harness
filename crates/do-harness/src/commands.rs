//! Subcommand dispatchers shared by the CLI entrypoint.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::cli::{ErrorsAction, HookAction, TaskAction, TraceAction};
use crate::doctor::describe_binary;
use crate::report::Format;
use crate::{config, errors, hooks, init, task, trace};

/// Dispatches audit-chain check and prints report.
pub async fn audit_chain_cmd(root: &Path, format: Format) -> Result<()> {
    let report = crate::audit::audit_chain(root).await?;
    match format {
        Format::Text => match report {
            crate::audit::ChainReport::Intact { count } => {
                println!("OK: Hash chain intact ({count} event(s) verified).");
                Ok(())
            }
            crate::audit::ChainReport::Tampered { seq } => {
                println!("FAIL: Hash chain tampered at event seq {seq}.");
                anyhow::bail!("Hash chain tampered at event seq {seq}");
            }
        },
        Format::Json => {
            let json = match report {
                crate::audit::ChainReport::Intact { count } => {
                    serde_json::json!({ "status": "intact", "count": count })
                }
                crate::audit::ChainReport::Tampered { seq } => {
                    serde_json::json!({ "status": "tampered", "seq": seq })
                }
            };
            println!("{json}");
            if report.is_intact() {
                Ok(())
            } else {
                anyhow::bail!("Hash chain tampered");
            }
        }
    }
}

/// Embedded compliance document (`docs/compliance.md`).
const COMPLIANCE_DOC: &str = include_str!("../../../docs/compliance.md");

/// Prints compliance mapping information with optional framework filtering.
pub fn print_compliance_filtered(framework: Option<&str>, format: Format) {
    let frameworks = vec![
        "OWASP Agentic Top 10",
        "NIST AI RMF 1.0",
        "EU AI Act",
        "SOC 2",
    ];
    let filtered_frameworks: Vec<&str> = if let Some(fw) = framework {
        frameworks
            .into_iter()
            .filter(|f| f.to_lowercase().contains(&fw.to_lowercase()))
            .collect()
    } else {
        frameworks
    };

    match format {
        Format::Text => {
            if let Some(fw) = framework {
                println!("Framework filter: {fw}\n");
            }
            println!("{COMPLIANCE_DOC}");
        }
        Format::Json => {
            let json = serde_json::json!({
                "doc": COMPLIANCE_DOC,
                "frameworks": filtered_frameworks,
                "selected_framework": framework
            });
            println!("{json}");
        }
    }
}

/// Prints CLI version information in the requested format.
pub fn print_version(format: Format) {
    let info = crate::version::VersionInfo::current();
    println!("{}", info.format(format));
}

/// Dispatches task-state actions.
pub async fn task_cmd(root: &Path, action: TaskAction) -> Result<()> {
    match action {
        TaskAction::Export {
            output,
            stdout,
            format,
        } => {
            let count = task::export_tasks(root, output.as_deref(), stdout, format).await?;
            if !stdout {
                let target = output.unwrap_or_else(|| PathBuf::from("plans/tasks.json"));
                println!("Exported {count} task(s) to {}", target.display());
            }
            Ok(())
        }
        TaskAction::List {
            status,
            method,
            parent,
            format,
        } => task::list_tasks(root, format, status.as_deref(), method.as_deref(), parent).await,
        TaskAction::Show { id, format } => task::show_task(root, id, format).await,
        TaskAction::Add {
            title,
            method,
            parent,
            precondition,
        } => {
            let (id, _event) = task::add_task(
                root,
                &title,
                method.as_deref(),
                parent,
                precondition.as_deref(),
            )
            .await?;
            println!("Added task {id}: {title}");
            Ok(())
        }
        TaskAction::Advance { id, dry_run } => {
            if dry_run {
                println!("Dry run: would advance task {id}");
                Ok(())
            } else {
                let (index, _event) = task::advance_task(root, id).await?;
                println!("Advanced task {id} to subtask_index={index}");
                Ok(())
            }
        }
        TaskAction::Done { id, dry_run } => {
            if dry_run {
                println!("Dry run: would mark task {id} done");
                Ok(())
            } else {
                task::done_task(root, id).await?;
                println!("Done task {id}");
                Ok(())
            }
        }
        TaskAction::Fail { id } => {
            task::fail_task(root, id).await?;
            println!("Failed task {id}");
            Ok(())
        }
        TaskAction::Remove { id } => task::remove_task(root, id).await,
    }
}

/// Dispatches trace actions.
pub async fn trace_cmd(root: &Path, action: TraceAction) -> Result<()> {
    match action {
        TraceAction::Add {
            session,
            task,
            command,
            error_diff,
            resolution_steps,
        } => {
            let opts = trace::TraceOpts {
                task_id: task,
                session: &session,
                command: command.as_deref(),
                error_diff: error_diff.as_deref(),
                resolution_steps: resolution_steps.as_deref(),
            };
            let id = trace::add_trace(root, &opts).await?;
            println!("Recorded trace {id} in session {session}");
            Ok(())
        }
        TraceAction::List { session, format } => trace::list_traces(root, &session, format).await,
        TraceAction::Sessions { format } => trace::list_sessions(root, format).await,
    }
}

/// Dispatches error-signature actions.
pub async fn errors_cmd(root: &Path, action: ErrorsAction) -> Result<()> {
    match action {
        ErrorsAction::List { task, format } => errors::list(root, task, format).await,
        ErrorsAction::Clear {
            sensor,
            task,
            force: _,
            dry_run,
        } => {
            if dry_run {
                println!("Dry run: would clear error signatures");
                return Ok(());
            }
            let key = sensor.as_deref().map(|s| {
                if s.starts_with("sensor:") {
                    s.to_owned()
                } else {
                    format!("sensor:{s}")
                }
            });
            let removed = errors::clear(root, task, key.as_deref()).await?;
            println!("Cleared {removed} error signature(s)");
            Ok(())
        }
    }
}

/// Dispatches hook management using the configured sensor split.
pub async fn hook(root: &Path, config_path: Option<&Path>, action: HookAction) -> Result<()> {
    let cwd = std::env::current_dir().context("failed to read current directory")?;
    let git_dir = hooks::find_git_dir(&cwd)?;
    match action {
        HookAction::Install { force } => {
            let cfg = config::load(root, config_path).await?;
            hooks::install(&git_dir, &cfg.hooks.pre_commit, &cfg.hooks.pre_push, force)?;
            println!("Installed managed git hooks in {}", git_dir.display());
        }
        HookAction::Uninstall => {
            hooks::uninstall(&git_dir)?;
            println!("Removed managed hooks from {}", git_dir.display());
        }
        HookAction::Status { format } => {
            let status = hooks::status(&git_dir, root);
            match format {
                Format::Text => {
                    println!(
                        "pre-commit: {}  pre-push: {}  commit-msg: {}  binary: {} ({})",
                        if status.pre_commit {
                            "installed"
                        } else {
                            "absent"
                        },
                        if status.pre_push {
                            "installed"
                        } else {
                            "absent"
                        },
                        if status.commit_msg {
                            "installed"
                        } else {
                            "absent"
                        },
                        describe_binary(&status.binary),
                        if status.binary.present() {
                            "present"
                        } else {
                            "missing"
                        }
                    );
                }
                Format::Json => {
                    let json = serde_json::json!({
                        "pre_commit": status.pre_commit,
                        "pre_push": status.pre_push,
                        "commit_msg": status.commit_msg,
                        "binary_present": status.binary.present(),
                        "binary_source": describe_binary(&status.binary)
                    });
                    println!("{json}");
                }
            }
        }
        HookAction::Diff => {
            let status = hooks::status(&git_dir, root);
            if status.pre_commit && status.pre_push && status.commit_msg {
                println!("Hooks match installed templates.");
            } else {
                println!("One or more hooks differ or are missing.");
            }
        }
    }
    Ok(())
}

/// Options for `init-db`.
#[derive(Debug, Clone, Copy)]
pub struct InitDbOpts {
    /// Report pending migrations and exit non-zero when any are pending.
    pub check: bool,
    /// Report migration state without applying anything (exit 0).
    pub dry_run: bool,
    /// Skip the interactive confirmation prompt.
    pub yes: bool,
}

/// Applies pending migrations and reports the number applied.
///
/// Interactive terminals confirm before applying pending migrations; hooks,
/// CI, and piped invocations apply without prompting (pass `--yes` to force
/// non-interactive behavior explicitly). `--check`/`--dry-run` never write.
///
/// # Errors
///
/// Returns an error when the health probe fails, `--check` sees an unmigrated
/// database, or applying migrations fails.
pub async fn init_db(root: &Path, opts: &InitDbOpts) -> Result<()> {
    use std::io::IsTerminal as _;

    let health = crate::dbcheck::probe(root).await?;
    let pending = matches!(health, crate::dbcheck::DbHealth::Pending { .. });
    if opts.check || opts.dry_run {
        let (mark, line) = health.render();
        println!("[{mark}] {line}");
        if opts.check && !matches!(health, crate::dbcheck::DbHealth::Current) {
            anyhow::bail!("database is not at the current migration version");
        }
        return Ok(());
    }
    if pending && !opts.yes && std::io::stdin().is_terminal() && !confirm_migrations(&health)? {
        anyhow::bail!("migration cancelled; re-run with --yes to apply");
    }
    let conn = do_harness_db::connect_and_migrate(root).await?;
    let count = do_harness_db::count_migrations(&conn).await?;
    println!("Done. Applied schema migrations: {count}");
    Ok(())
}

/// Prompts on the terminal for confirmation to apply pending migrations.
fn confirm_migrations(health: &crate::dbcheck::DbHealth) -> Result<bool> {
    use std::io::{BufRead as _, Write as _};

    let (_, line) = health.render();
    print!("{line}\nApply pending migrations now? [y/N] ");
    std::io::stdout()
        .flush()
        .context("failed to flush prompt")?;
    let mut answer = String::new();
    std::io::stdin()
        .lock()
        .read_line(&mut answer)
        .context("failed to read confirmation")?;
    Ok(matches!(
        answer.trim().to_ascii_lowercase().as_str(),
        "y" | "yes"
    ))
}

/// Prunes old beats and compacts the state database.
///
/// `--prune-beats <days>` deletes beats older than the cutoff while keeping at
/// least `keep_per_task` most-recent beats per task; the database is then
/// `VACUUM`-compacted. Without the flag only `VACUUM` runs.
///
/// # Errors
///
/// Returns an error when the database cannot be opened or pruned.
pub async fn maintenance(root: &Path, prune_beats: Option<i64>, keep_per_task: i64) -> Result<()> {
    let conn = do_harness_db::connect_and_migrate(root).await?;
    if let Some(days) = prune_beats {
        let cutoff = do_harness_db::unix_now().saturating_sub(days.max(0).saturating_mul(86_400));
        let deleted = do_harness_db::prune_beats(&conn, cutoff, keep_per_task.max(0)).await?;
        println!(
            "Pruned {deleted} beat(s) older than {days} day(s) (kept >= {keep_per_task} per task)"
        );
    }
    do_harness_db::vacuum(&conn).await?;
    println!("VACUUM complete");
    Ok(())
}

/// Seeds the `invariants` table from `plans/invariants.json`.
pub async fn seed(root: &Path) -> Result<()> {
    let written = crate::init::seed_invariants(root).await?;
    println!(
        "Seeded {written} invariants from {}",
        root.join("plans/invariants.json").display()
    );
    Ok(())
}

/// Resolves the workspace root: explicit override or walk up from cwd.
pub fn resolve_root(explicit: Option<&Path>) -> Result<PathBuf> {
    if let Some(path) = explicit {
        if !path.is_dir() {
            anyhow::bail!("root is not a directory: {}", path.display());
        }
        Ok(path.to_path_buf())
    } else {
        let cwd = std::env::current_dir().context("failed to read current directory")?;
        Ok(do_harness_db::find_harness_root(&cwd)?)
    }
}

/// Resolves the target directory for `init`: explicit root or cwd.
pub fn init_target(explicit: Option<&Path>) -> Result<PathBuf> {
    if let Some(path) = explicit {
        if !path.is_dir() {
            anyhow::bail!("root is not a directory: {}", path.display());
        }
        return Ok(path.to_path_buf());
    }
    std::env::current_dir().context("failed to read current directory")
}

/// Prints the init report and next steps.
pub fn print_init(report: &init::InitReport, root: &Path, language: init::Language) {
    println!("Initialized do-harness workspace in {}", root.display());
    for path in &report.written {
        println!("  wrote {path}");
    }
    for path in &report.skipped {
        println!("  skipped {path} (exists; re-run with --force to overwrite)");
    }
    println!("Seeded {} invariants.", report.seeded);
    println!();
    println!("Next steps:");
    println!("  do-harness hook install   # wire git hooks");
    println!("  do-harness list           # show the configured sensors");
    match language {
        init::Language::Rust => {
            println!(
                "  do-harness verify         # full rust suite; a fresh init scaffolded the crate to verify"
            );
        }
        init::Language::Generic => {
            println!(
                "  do-harness verify         # NOTE: zero sensors — a pass is vacuous until you add [[sensors]]"
            );
        }
    }
}
