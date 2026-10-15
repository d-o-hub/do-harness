//! `doctor` subcommand: binary resolution, git hook health, and state-database
//! migration skew.

use std::path::Path;

use anyhow::Result;

use crate::dbcheck;
use crate::hook_script::BinSource;
use crate::hooks;
use crate::report::Format;

/// Outcome of the event-chain tamper audit, computed once so both output
/// formats report it. Before this existed the audit ran only in the text
/// branch, so `--format json` reported `ok: true` on a tampered chain.
enum ChainStatus {
    /// No state database is present, so there is nothing to audit.
    Absent,
    /// The chain verified end to end.
    Intact { count: i64 },
    /// The first divergence was found at `seq`.
    Tampered { seq: i64 },
    /// The chain could not be read.
    Unreadable,
}

impl ChainStatus {
    fn to_json(&self) -> serde_json::Value {
        match self {
            Self::Absent => serde_json::Value::Null,
            Self::Intact { count } => serde_json::json!({ "status": "intact", "count": count }),
            Self::Tampered { seq } => serde_json::json!({ "status": "tampered", "seq": seq }),
            Self::Unreadable => serde_json::json!({ "status": "unreadable" }),
        }
    }
}

/// Runs diagnostic checks covering binary resolution, git hook status, and
/// state-database migration skew.
pub async fn run(root: &Path, format: Format, strict: bool) -> Result<()> {
    let git_dir = hooks::find_git_dir(root)?;
    let status = hooks::status(&git_dir, root);
    run_with_status(root, format, strict, &status).await
}

/// Reads the task-health counters used by the doctor's event-log checks.
///
/// # Errors
///
/// Returns an error when the state database cannot be connected or queried;
/// the caller records that as a failure instead of the all-clear.
async fn read_task_health(root: &Path) -> Result<(i64, Option<i64>)> {
    let conn = do_harness_db::connect_and_migrate(root).await?;
    let orphans = do_harness_db::count_tasks_without_added_event(&conn).await?;
    let latest = do_harness_db::latest_task_update(&conn).await?;
    Ok((orphans, latest))
}

/// Core diagnostics over an already-resolved hook/binary status.
///
/// Split from [`run`] so tests are hermetic: binary resolution reads
/// `DO_HARNESS_BIN`/`PATH`, and a test that expects a missing binary must not
/// inherit the caller's resolved binary.
#[allow(clippy::too_many_lines)]
pub(crate) async fn run_with_status(
    root: &Path,
    format: Format,
    strict: bool,
    status: &hooks::HookStatus,
) -> Result<()> {
    let mut failures: Vec<String> = Vec::new();

    let bin_path = status.binary.path();
    let bin_source_desc = describe_binary(&status.binary);
    let bin_ok = status.binary.present();

    if !bin_ok {
        failures.push(format!("binary missing at {}", bin_path.display()));
    }

    if strict && status.binary.is_in_target_dir() {
        failures.push(format!(
            "binary is under target/ in strict mode: {}",
            bin_path.display()
        ));
    }

    if status.is_shadowed() {
        let cfg = status.core_hooks_path.as_deref().unwrap_or("configured");
        failures.push(format!(
            "one or more managed git hooks shadowed by core.hooksPath={cfg}"
        ));
    }

    let hooks_installed = status.pre_commit && status.pre_push && status.commit_msg;
    if strict && !hooks_installed {
        failures.push("one or more managed git hooks absent in strict mode".to_owned());
    }

    let db_res = dbcheck::probe(root).await;
    let db_ok = match &db_res {
        Ok(health) => !health.is_blocking(),
        Err(_) => false,
    };
    if let Err(e) = &db_res {
        failures.push(format!("state database unreadable: {e:#}"));
    } else if let Ok(health) = &db_res {
        if health.is_blocking() {
            let (_, line) = health.render();
            failures.push(line);
        }
    }

    // Out-of-band task writes leave tasks with no TaskAdded event; they break
    // the append-only workflow log and would otherwise go unnoticed. A read
    // failure must not masquerade as the all-clear: it is recorded as a
    // failure and the counts serialize as null instead of zero.
    let task_health = if do_harness_db::db_path(root).exists() {
        match read_task_health(root).await {
            Ok(pair) => Some(pair),
            Err(err) => {
                failures.push(format!("task/event checks unreadable: {err:#}"));
                None
            }
        }
    } else {
        Some((0, None))
    };
    let (orphan_tasks, stale_export) = match task_health {
        Some((orphans, latest_task_update)) => {
            if orphans > 0 {
                failures.push(format!(
                    "{orphans} task(s) have no TaskAdded event (out-of-band write)"
                ));
            }
            let stale = check_task_export(root, latest_task_update, &mut failures).await;
            (Some(orphans), Some(stale))
        }
        None => (None, None),
    };

    // The event-chain audit is tamper evidence and must run in BOTH output
    // modes: it used to live only in the text branch, so `--format json`
    // assembled and returned its report before the audit ran, reporting
    // `ok: true` on a tampered chain.
    let chain = if do_harness_db::db_path(root).exists() {
        match crate::audit::audit_chain(root).await {
            Ok(crate::audit::ChainReport::Intact { count }) => ChainStatus::Intact { count },
            Ok(crate::audit::ChainReport::Tampered { seq }) => {
                failures.push(format!("event chain tampered at seq {seq}"));
                ChainStatus::Tampered { seq }
            }
            Err(err) => {
                failures.push(format!("event chain unreadable: {err:#}"));
                ChainStatus::Unreadable
            }
        }
    } else {
        ChainStatus::Absent
    };

    if format == Format::Json {
        let json = serde_json::json!({
            "ok": failures.is_empty(),
            "binary": {
                "present": bin_ok,
                "source": bin_source_desc,
                "path": bin_path.display().to_string(),
                "in_target_dir": status.binary.is_in_target_dir()
            },
            "hooks": {
                "pre_commit": status.pre_commit,
                "pre_push": status.pre_push,
                "commit_msg": status.commit_msg,
                "pre_commit_shadowed": status.pre_commit_shadowed,
                "pre_push_shadowed": status.pre_push_shadowed,
                "commit_msg_shadowed": status.commit_msg_shadowed,
                "shadowed": status.is_shadowed()
            },
            "database_ok": db_ok,
            "event_chain": chain.to_json(),
            "orphan_tasks": orphan_tasks,
            "stale_task_export": stale_export,
            "failures": failures
        });
        println!("{json}");
    } else {
        println!("Doctor Summary:");
        println!("===============");

        if bin_ok {
            println!(
                "  [OK] Binary resolution: {bin_source_desc} ({})",
                bin_path.display()
            );
        } else {
            println!(
                "  [FAIL] Binary resolution: {bin_source_desc} ({}) - file missing",
                bin_path.display()
            );
        }

        if status.binary.is_in_target_dir() {
            eprintln!(
                "warning: resolved do-harness binary is under Cargo target/; cargo clean will remove it: {}",
                bin_path.display()
            );
        }

        for (name, installed, shadowed) in [
            ("pre-commit", status.pre_commit, status.pre_commit_shadowed),
            ("pre-push", status.pre_push, status.pre_push_shadowed),
            ("commit-msg", status.commit_msg, status.commit_msg_shadowed),
        ] {
            if installed {
                println!("  [OK] {name} hook: installed");
            } else if shadowed {
                let cfg = status.core_hooks_path.as_deref().unwrap_or("configured");
                println!(
                    "  [FAIL] {name} hook: installed in .git/hooks but shadowed by core.hooksPath={cfg}"
                );
            } else {
                println!("  [WARN] {name} hook: absent");
            }
        }

        match &db_res {
            Ok(health) => {
                let (mark, line) = health.render();
                println!("  [{mark}] state database: {line}");
            }
            Err(err) => {
                println!("  [FAIL] state database: unreadable ({err:#})");
            }
        }

        match orphan_tasks {
            Some(0) => println!("  [OK] event log: no orphan tasks"),
            Some(count) => println!(
                "  [FAIL] event log: {count} task(s) without a TaskAdded event (out-of-band write)"
            ),
            None => println!("  [FAIL] event log: unreadable (see failures)"),
        }

        match stale_export {
            Some(true) => println!("  [FAIL] task export: plans/tasks.json lags the database"),
            Some(false) => println!("  [OK] task export: up to date or absent"),
            None => println!("  [FAIL] task export: unreadable (see failures)"),
        }

        match &chain {
            ChainStatus::Absent => {}
            ChainStatus::Intact { count } => {
                println!("  [OK] event chain: intact ({count} event(s))");
            }
            ChainStatus::Tampered { seq } => {
                println!("  [FAIL] event chain: tampered at seq {seq}");
            }
            ChainStatus::Unreadable => {
                println!("  [FAIL] event chain: unreadable (see failures)");
            }
        }

        if failures.is_empty() {
            println!("\nDoctor checks passed.");
        }
    }

    if failures.is_empty() {
        Ok(())
    } else {
        anyhow::bail!("doctor check failed: {}", failures.join("; "))
    }
}

pub(crate) fn describe_binary(source: &BinSource) -> String {
    match source {
        BinSource::Env(path) => format!("env:{}", path.display()),
        BinSource::Path(path) => format!("path:{}", path.display()),
        BinSource::Repo(path) => format!("repo:{}", path.display()),
    }
}

/// Flags `plans/tasks.json` when it predates the newest task update.
///
/// Returns `true` when the export is missing, unreadable, invalid, or stale;
/// failures are also appended to `failures`.
async fn check_task_export(
    root: &Path,
    latest_task_update: Option<i64>,
    failures: &mut Vec<String>,
) -> bool {
    let path = root.join("plans/tasks.json");
    if !path.exists() {
        return false;
    }
    let raw = match tokio::fs::read_to_string(&path).await {
        Ok(raw) => raw,
        Err(err) => {
            failures.push(format!("plans/tasks.json unreadable: {err}"));
            return true;
        }
    };
    match serde_json::from_str::<crate::task::TaskSnapshot>(&raw) {
        Ok(snapshot) => {
            if latest_task_update.is_some_and(|updated| updated > snapshot.exported_at) {
                failures.push(
                    "plans/tasks.json export lags the database (run: do-harness task export)"
                        .to_owned(),
                );
                true
            } else {
                false
            }
        }
        Err(err) => {
            failures.push(format!("plans/tasks.json invalid: {err}"));
            true
        }
    }
}

#[cfg(test)]
#[path = "doctor_tests.rs"]
mod tests;
