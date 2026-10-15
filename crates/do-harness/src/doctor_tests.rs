//! Unit tests for the `doctor` subcommand (`doctor.rs`).

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

/// The repo-local release path the resolver expects on this platform
/// (Cargo appends `.exe` on Windows).
fn release_binary(root: &Path) -> PathBuf {
    let mut bin_path = root.join("target/release/do-harness");
    if cfg!(windows) {
        bin_path.set_extension("exe");
    }
    bin_path
}

fn stub_binary(root: &Path) {
    let bin_path = release_binary(root);
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
        pre_commit_shadowed: false,
        pre_push_shadowed: false,
        commit_msg_shadowed: false,
        binary: BinSource::Repo(release_binary(root)),
        core_hooks_path: None,
        hooks_dir: root.join(".git/hooks"),
    }
}

#[tokio::test(flavor = "current_thread")]
async fn fails_when_hooks_are_shadowed() {
    let (_temp, root) = fake_repo_with_git();
    stub_binary(&root);
    let mut status = repo_status(&root);
    status.pre_commit = false;
    status.pre_commit_shadowed = true;
    status.core_hooks_path = Some(".githooks".to_string());

    let result = run_with_status(&root, Format::Text, false, &status).await;

    let message = result.unwrap_err().to_string();
    assert!(message.contains("doctor check failed"));
    assert!(message.contains("shadowed by core.hooksPath=.githooks"));
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

/// A tampered event hash chain is a hard failure, matching `audit-chain`.
#[tokio::test(flavor = "current_thread")]
async fn fails_on_tampered_event_chain() {
    let (_temp, root) = fake_repo_with_git();
    stub_binary(&root);
    let conn = do_harness_db::connect_and_migrate(&root).await.unwrap();
    do_harness_db::insert_task_with_event(
        &conn,
        &do_harness_db::NewTask {
            title: "tampered",
            method: Some("mini"),
            subtask_index: 0,
            precondition: None,
            parent_id: None,
        },
    )
    .await
    .unwrap();
    conn.execute(
        "UPDATE workflow_events SET payload = '{}' WHERE seq = 1",
        (),
    )
    .await
    .unwrap();
    drop(conn);

    let message = run_with_status(&root, Format::Text, false, &repo_status(&root))
        .await
        .unwrap_err()
        .to_string();
    assert!(message.contains("tampered"), "{message}");
}

/// The same tampered chain must fail under `--format json` too. The audit
/// used to run only in the text branch, so JSON reported `"ok": true` (and
/// exited 0) while text failed.
#[tokio::test(flavor = "current_thread")]
async fn fails_on_tampered_event_chain_json() {
    let (_temp, root) = fake_repo_with_git();
    stub_binary(&root);
    let conn = do_harness_db::connect_and_migrate(&root).await.unwrap();
    do_harness_db::insert_task_with_event(
        &conn,
        &do_harness_db::NewTask {
            title: "tampered",
            method: Some("mini"),
            subtask_index: 0,
            precondition: None,
            parent_id: None,
        },
    )
    .await
    .unwrap();
    conn.execute(
        "UPDATE workflow_events SET payload = '{}' WHERE seq = 1",
        (),
    )
    .await
    .unwrap();
    drop(conn);

    let message = run_with_status(&root, Format::Json, false, &repo_status(&root))
        .await
        .unwrap_err()
        .to_string();
    assert!(message.contains("tampered"), "{message}");
}
