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
#[allow(clippy::too_many_lines)]
pub async fn run(root: &Path, format: Format, strict: bool) -> Result<()> {
    let git_dir = hooks::find_git_dir(root)?;
    let status = hooks::status(&git_dir, root);

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
    let orphan_tasks = if do_harness_db::db_path(root).exists() {
        match do_harness_db::connect_and_migrate(root).await {
            Ok(conn) => do_harness_db::count_tasks_without_added_event(&conn)
                .await
                .unwrap_or(0),
            Err(_) => 0,
        }
    } else {
        0
    };
    if orphan_tasks > 0 {
        failures.push(format!(
            "{orphan_tasks} task(s) have no TaskAdded event (out-of-band write)"
        ));
    }

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

    #[tokio::test(flavor = "current_thread")]
    async fn fails_when_binary_missing_and_names_the_reason() {
        let (_temp, root) = fake_repo_with_git();

        let result = run(&root, Format::Text, false).await;

        let message = result.unwrap_err().to_string();
        assert!(message.contains("doctor check failed"));
        assert!(message.contains("binary missing"));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn succeeds_when_binary_present_and_database_absent() {
        let (_temp, root) = fake_repo_with_git();
        stub_binary(&root);

        assert!(run(&root, Format::Text, false).await.is_ok());
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

        let result = run(&root, Format::Text, false).await;

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

        let message = run(&root, Format::Text, false)
            .await
            .unwrap_err()
            .to_string();
        assert!(message.contains("no TaskAdded event"), "{message}");
    }
}
