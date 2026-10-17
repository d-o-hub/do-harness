//! Subcommand dispatchers shared by the CLI entrypoint.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::cli::{ErrorsAction, TaskAction, TraceAction};
use crate::report::Format;
use crate::{errors, task, trace};

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
///
/// The asset is a symlink to the canonical document so the two can never
/// drift; `cargo package` dereferences it into the published crate.
const COMPLIANCE_DOC: &str = include_str!("../assets/compliance.md");

/// Prints compliance mapping information with optional framework filtering.
pub fn print_compliance_filtered(framework: Option<&str>, format: Format) {
    let frameworks = vec![
        "OWASP Agentic Top 10",
        "OWASP LLM Top 10",
        "NIST AI RMF 1.0",
        "EU AI Act",
        "SOC 2",
    ];
    // Compare on lowercase alphanumerics only, so the hyphenated ids the help
    // advertises (`owasp-agentic-top10`, `nist-ai-rmf`, `eu-ai-act`, `soc2`)
    // match the spaced display names as well as `owasp`/`nist`/`eu`/`soc` do.
    let normalize = |value: &str| -> String {
        value
            .to_lowercase()
            .chars()
            .filter(char::is_ascii_alphanumeric)
            .collect()
    };
    let filtered_frameworks: Vec<&str> = if let Some(fw) = framework {
        let needle = normalize(fw);
        frameworks
            .into_iter()
            .filter(|f| !needle.is_empty() && normalize(f).contains(&needle))
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
///
/// `dry_run` is the global `--dry-run` flag, threaded in rather than declared
/// per subcommand so one switch governs every mutating action: `add`, `advance`,
/// `done`, `fail`, and `remove` are all no-ops under it. `add` previously read
/// no dry-run state at all and wrote the row anyway while the flag claimed
/// "without side effects".
pub async fn task_cmd(root: &Path, action: TaskAction, dry_run: bool) -> Result<()> {
    match action {
        TaskAction::Export {
            output,
            stdout,
            format,
        } => {
            if dry_run {
                println!("Dry run: would export the task board");
                return Ok(());
            }
            let count = task::export_tasks(root, output.as_deref(), stdout, format).await?;
            if !stdout {
                let target = output.unwrap_or_else(|| PathBuf::from("plans/tasks.json"));
                println!("Exported {count} task(s) to {}", target.display());
            }
            Ok(())
        }
        TaskAction::Import { file, check } => {
            task::import_tasks(root, file.as_deref(), check).await
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
            if dry_run {
                println!("Dry run: would add task: {title}");
                return Ok(());
            }
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
        TaskAction::Advance { id, .. } => {
            if dry_run {
                task::preview_advance(root, id).await?;
                println!("Dry run: would advance task {id}");
                return Ok(());
            }
            let (index, _event) = task::advance_task(root, id).await?;
            println!("Advanced task {id} to subtask_index={index}");
            Ok(())
        }
        TaskAction::Done { id, .. } => {
            if dry_run {
                task::preview_done(root, id).await?;
                println!("Dry run: would mark task {id} done");
                return Ok(());
            }
            task::done_task(root, id).await?;
            println!("Done task {id}");
            Ok(())
        }
        TaskAction::Fail { id } => {
            if dry_run {
                task::preview_fail(root, id).await?;
                println!("Dry run: would mark task {id} failed");
                return Ok(());
            }
            task::fail_task(root, id).await?;
            println!("Failed task {id}");
            Ok(())
        }
        TaskAction::Remove { id } => {
            if dry_run {
                task::preview_remove(root, id).await?;
                println!("Dry run: would remove task {id}");
                return Ok(());
            }
            task::remove_task(root, id).await
        }
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
        ErrorsAction::List {
            task,
            scope,
            format,
        } => {
            errors::list(
                root,
                scope_filter(scope.as_deref(), task).as_deref(),
                format,
            )
            .await
        }
        ErrorsAction::Clear {
            sensor,
            task,
            scope,
            force,
            dry_run,
        } => {
            if sensor.is_none() && task.is_none() && scope.is_none() && !force {
                anyhow::bail!(
                    "refusing to clear every workstream; pass --sensor, --task or --scope (or --force to clear all)"
                );
            }
            let key = sensor.as_deref().map(|s| {
                if s.starts_with("sensor:") {
                    s.to_owned()
                } else {
                    format!("sensor:{s}")
                }
            });
            let scope_key = scope_filter(scope.as_deref(), task);
            let target = match (&scope_key, &key) {
                (Some(s), Some(k)) => format!("scope {s}, {k}"),
                (Some(s), None) => format!("scope {s}"),
                (None, Some(k)) => format!("{k} across ALL scopes"),
                (None, None) => "ALL scopes".to_owned(),
            };
            if dry_run {
                println!("Dry run: would clear error signatures for {target}");
                return Ok(());
            }
            let removed = errors::clear(root, scope_key.as_deref(), key.as_deref()).await?;
            println!("Cleared {removed} error signature(s) for {target}");
            Ok(())
        }
    }
}

/// Resolves the workstream filter from `--scope`/`--task`: `all` (or no flag)
/// means every workstream, `--task <ID>` means `task:<ID>`.
fn scope_filter(scope: Option<&str>, task: Option<i64>) -> Option<String> {
    match scope {
        Some("all") => None,
        Some(raw) => Some(raw.to_owned()),
        None => task.map(|id| format!("task:{id}")),
    }
}

mod db;
pub use db::{InitDbOpts, init_db, maintenance, seed};
mod hook;
pub use hook::hook;
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
