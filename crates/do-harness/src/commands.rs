//! Subcommand dispatchers shared by the CLI entrypoint.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::doctor::describe_binary;
use crate::report::Format;
use crate::{ErrorsAction, TaskAction, TraceAction, config, errors, hooks, init, task, trace};

/// Embedded compliance document (`docs/compliance.md`).
const COMPLIANCE_DOC: &str = include_str!("../../../docs/compliance.md");

/// Prints compliance mapping information in text or JSON format.
pub fn print_compliance(format: Format) {
    match format {
        Format::Text => println!("{COMPLIANCE_DOC}"),
        Format::Json => {
            let json = serde_json::json!({
                "doc": COMPLIANCE_DOC,
                "frameworks": ["OWASP Agentic Top 10", "NIST AI RMF 1.0", "EU AI Act"]
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
///
/// # Errors
///
/// Returns an error when the state database cannot be opened or the export
/// cannot be written.
pub async fn task_cmd(root: &Path, action: TaskAction) -> Result<()> {
    match action {
        TaskAction::Export => {
            let count = task::export_tasks(root).await?;
            println!("Exported {count} task(s) to plans/tasks.json");
            Ok(())
        }
        TaskAction::List { format } => task::list_tasks(root, format).await,
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
        TaskAction::Advance { id } => {
            let (index, _event) = task::advance_task(root, id).await?;
            println!("Advanced task {id} to subtask_index={index}");
            Ok(())
        }
        TaskAction::Done { id } => {
            task::done_task(root, id).await?;
            println!("Done task {id}");
            Ok(())
        }
        TaskAction::Fail { id } => {
            task::fail_task(root, id).await?;
            println!("Failed task {id}");
            Ok(())
        }
    }
}

/// Dispatches trace actions.
///
/// # Errors
///
/// Returns an error when the state database cannot be opened.
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
    }
}

/// Dispatches error-signature actions.
///
/// # Errors
///
/// Returns an error when the state database cannot be opened.
pub async fn errors_cmd(root: &Path, action: ErrorsAction) -> Result<()> {
    match action {
        ErrorsAction::List { task, format } => errors::list(root, task, format).await,
        ErrorsAction::Clear { sensor, task } => {
            let removed = errors::clear(root, task, sensor.as_deref()).await?;
            println!("Cleared {removed} error signature(s)");
            Ok(())
        }
    }
}

/// Dispatches hook management using the configured sensor split.
///
/// # Errors
///
/// Returns an error when no git repository is found or a hook file cannot be
/// written or removed.
pub fn hook(root: &Path, config_path: Option<&Path>, action: crate::HookAction) -> Result<()> {
    let cwd = std::env::current_dir().context("failed to read current directory")?;
    let git_dir = hooks::find_git_dir(&cwd)?;
    match action {
        crate::HookAction::Install { force } => {
            let cfg = config::load(root, config_path)?;
            hooks::install(&git_dir, &cfg.hooks.pre_commit, &cfg.hooks.pre_push, force)?;
            println!(
                "Installed pre-commit, pre-push, and commit-msg hooks in {}",
                git_dir.display()
            );
        }
        crate::HookAction::Uninstall => {
            hooks::uninstall(&git_dir)?;
            println!("Removed managed hooks from {}", git_dir.display());
        }
        crate::HookAction::Status => {
            let status = hooks::status(&git_dir, root);
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
            if status.binary.is_in_target_dir() {
                eprintln!(
                    "warning: resolved do-harness binary is under Cargo target/; cargo clean will remove it: {}",
                    status.binary.path().display()
                );
            }
        }
    }
    Ok(())
}

/// Applies pending migrations and reports the number applied.
pub async fn init_db(root: &Path) -> Result<()> {
    let conn = do_harness_db::connect_and_migrate(root).await?;
    let mut rows = conn
        .query("SELECT COUNT(*) FROM schema_migrations", ())
        .await?;
    let count = match rows.next().await? {
        Some(row) => row.get::<i64>(0)?,
        None => anyhow::bail!("schema_migrations is unexpectedly empty"),
    };
    println!("Done. Applied schema migrations: {count}");
    Ok(())
}

/// Seeds the `invariants` table from `plans/invariants.json`.
pub async fn seed(root: &Path) -> Result<()> {
    let json_path = root.join("plans/invariants.json");
    let json = std::fs::read_to_string(&json_path)
        .with_context(|| format!("failed to read {}", json_path.display()))?;
    let headers: Vec<do_harness_types::DecisionHeader> = serde_json::from_str(&json)
        .context("invalid invariants.json: does not match DecisionHeader schema")?;

    let conn = do_harness_db::connect_and_migrate(root).await?;
    let written = do_harness_db::seed_invariants(&conn, &headers).await?;

    println!("Seeded {written} invariants from {}", json_path.display());
    Ok(())
}

/// Resolves the workspace root: explicit override or walk up from cwd.
///
/// # Errors
///
/// Returns an error when the explicit root is not a directory or no harness
/// root can be discovered from the current directory.
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
///
/// Unlike [`resolve_root`], this does not require an existing harness root so
/// fresh repositories can be scaffolded.
///
/// # Errors
///
/// Returns an error when the explicit root is not a directory.
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
