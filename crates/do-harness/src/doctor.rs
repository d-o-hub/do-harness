//! `doctor` subcommand: binary resolution, git hook health, and state-database
//! migration skew.

use std::path::Path;

use anyhow::Result;

use crate::dbcheck;
use crate::hook_script::BinSource;
use crate::hooks;
use crate::report::Format;

/// Runs diagnostic checks covering binary resolution, git hook status, and
/// state-database migration skew.
pub async fn run(root: &Path, format: Format, strict: bool) -> Result<()> {
    let git_dir = hooks::find_git_dir(root)?;
    let status = hooks::status(&git_dir, root);
    run_with_status(root, format, strict, &status).await
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
    // the append-only workflow log and would otherwise go unnoticed.
    let (orphan_tasks, latest_task_update) = if do_harness_db::db_path(root).exists() {
        match do_harness_db::connect_and_migrate(root).await {
            Ok(conn) => (
                do_harness_db::count_tasks_without_added_event(&conn)
                    .await
                    .unwrap_or(0),
                do_harness_db::latest_task_update(&conn)
                    .await
                    .unwrap_or(None),
            ),
            Err(_) => (0, None),
        }
    } else {
        (0, None)
    };
    if orphan_tasks > 0 {
        failures.push(format!(
            "{orphan_tasks} task(s) have no TaskAdded event (out-of-band write)"
        ));
    }
    let stale_export = check_task_export(root, latest_task_update, &mut failures).await;

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
            },
            "database_ok": db_ok,
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

        for (name, installed) in [
            ("pre-commit", status.pre_commit),
            ("pre-push", status.pre_push),
            ("commit-msg", status.commit_msg),
        ] {
            println!(
                "  [{}] {name} hook: {}",
                if installed { "OK" } else { "WARN" },
                if installed { "installed" } else { "absent" }
            );
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

        if orphan_tasks > 0 {
            println!(
                "  [FAIL] event log: {orphan_tasks} task(s) without a TaskAdded event (out-of-band write)"
            );
        } else {
            println!("  [OK] event log: no orphan tasks");
        }

        if stale_export {
            println!("  [FAIL] task export: plans/tasks.json lags the database");
        } else {
            println!("  [OK] task export: up to date or absent");
        }

        if do_harness_db::db_path(root).exists() {
            match crate::audit::audit_chain(root).await {
                Ok(report) => match report {
                    crate::audit::ChainReport::Intact { count } => {
                        println!("  [OK] event chain: intact ({count} event(s))");
                    }
                    crate::audit::ChainReport::Tampered { seq } => {
                        println!("  [WARN] event chain: tampered at seq {seq}");
                    }
                },
                Err(err) => {
                    println!("  [WARN] event chain: unreadable ({err:#})");
                }
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
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use std::fs;
    use std::path::PathBuf;

    fn fake_repo_with_git() -> (tempfile::TempDir, PathBuf) {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().to_path_buf();
        let git_dir = root.join(".git");
        fs::create_dir_all(git_dir.join("hooks")).unwrap();
        (temp, root)
    }

    fn stub_binary(root: &Path) {
        let bin_path = root.join("target/release/do-harness");
        fs::create_dir_all(bin_path.parent().unwrap()).unwrap();
        fs::write(&bin_path, "stub").unwrap();
    }

    /// Hook status pinned to the repository-local binary path, independent of
    /// the caller's `DO_HARNESS_BIN`/`PATH`.
    fn repo_status(root: &Path) -> hooks::HookStatus {
        hooks::HookStatus {
            pre_commit: true,
            pre_push: true,
            commit_msg: true,
            binary: BinSource::Repo(root.join("target/release/do-harness")),
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn fails_when_binary_missing_and_names_the_reason() {
        let (_temp, root) = fake_repo_with_git();

        let result = run_with_status(&root, Format::Text, false, &repo_status(&root)).await;

        let message = result.unwrap_err().to_string();
        assert!(message.contains("doctor check failed"));
        assert!(message.contains("binary missing"));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn succeeds_when_binary_present_and_database_absent() {
        let (_temp, root) = fake_repo_with_git();
        stub_binary(&root);

        assert!(
            run_with_status(&root, Format::Text, false, &repo_status(&root))
                .await
                .is_ok()
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn fails_when_state_database_is_future() {
        let (_temp, root) = fake_repo_with_git();
        stub_binary(&root);
        let conn = do_harness_db::connect_and_migrate(&root).await.unwrap();
        conn.execute(
            "INSERT INTO schema_migrations (version, name, applied_at) VALUES (9999, 'future', 0)",
            (),
        )
        .await
        .unwrap();
        drop(conn);

        let result = run_with_status(&root, Format::Text, false, &repo_status(&root)).await;

        let message = result.unwrap_err().to_string();
        assert!(message.contains("doctor check failed"));
        assert!(message.contains("rebuild"));
    }

    /// Out-of-band writes that leave a task without its `TaskAdded` event are
    /// surfaced as a hard doctor failure.
    #[tokio::test(flavor = "current_thread")]
    async fn fails_on_orphan_task_without_added_event() {
        let (_temp, root) = fake_repo_with_git();
        stub_binary(&root);
        let conn = do_harness_db::connect_and_migrate(&root).await.unwrap();
        let (id, _) = do_harness_db::insert_task_with_event(
            &conn,
            &do_harness_db::NewTask {
                title: "orphan",
                method: Some("mini"),
                subtask_index: 0,
                precondition: None,
                parent_id: None,
            },
        )
        .await
        .unwrap();
        conn.execute("DELETE FROM workflow_events WHERE task_id = ?1", [id])
            .await
            .unwrap();
        drop(conn);

        let message = run_with_status(&root, Format::Text, false, &repo_status(&root))
            .await
            .unwrap_err()
            .to_string();
        assert!(message.contains("no TaskAdded event"), "{message}");
    }

    /// A `plans/tasks.json` older than the newest task update is a failure.
    #[tokio::test(flavor = "current_thread")]
    async fn fails_on_stale_task_export() {
        let (_temp, root) = fake_repo_with_git();
        stub_binary(&root);
        let conn = do_harness_db::connect_and_migrate(&root).await.unwrap();
        do_harness_db::insert_task_with_event(
            &conn,
            &do_harness_db::NewTask {
                title: "fresh",
                method: Some("mini"),
                subtask_index: 0,
                precondition: None,
                parent_id: None,
            },
        )
        .await
        .unwrap();
        drop(conn);
        let plans = root.join("plans");
        std::fs::create_dir_all(&plans).unwrap();
        std::fs::write(
            plans.join("tasks.json"),
            r#"{"exported_at":0,"tasks":[],"summary":{"pending":0,"in_progress":0,"done":0,"failed":0}}"#,
        )
        .unwrap();

        let message = run_with_status(&root, Format::Text, false, &repo_status(&root))
            .await
            .unwrap_err()
            .to_string();
        assert!(message.contains("lags the database"), "{message}");
    }
}
