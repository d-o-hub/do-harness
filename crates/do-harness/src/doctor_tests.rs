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
        ownership: hooks::HookOwnership {
            pre_commit: HookOwner::Managed,
            pre_push: HookOwner::Managed,
            commit_msg: HookOwner::Managed,
        },
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
    status.ownership.pre_commit = HookOwner::Absent;
    status.pre_commit_shadowed = true;
    status.core_hooks_path = Some(".githooks".to_string());

    let result = run_with_status(&root, Format::Text, false, &status, None, false).await;

    let message = result.unwrap_err().to_string();
    assert!(message.contains("doctor check failed"));
    assert!(message.contains("shadowed by core.hooksPath=.githooks"));
}

#[tokio::test(flavor = "current_thread")]
async fn fails_when_binary_missing_and_names_the_reason() {
    let (_temp, root) = fake_repo_with_git();

    let result =
        run_with_status(&root, Format::Text, false, &repo_status(&root), None, false).await;

    let message = result.unwrap_err().to_string();
    assert!(message.contains("doctor check failed"));
    assert!(message.contains("binary missing"));
}

#[tokio::test(flavor = "current_thread")]
async fn succeeds_when_binary_present_and_database_absent() {
    let (_temp, root) = fake_repo_with_git();
    stub_binary(&root);
    fs::create_dir_all(root.join("scripts")).unwrap();
    fs::write(
        root.join("scripts/check-commitlint.sh"),
        "#!/usr/bin/env bash\n",
    )
    .unwrap();

    assert!(
        run_with_status(&root, Format::Text, false, &repo_status(&root), None, false)
            .await
            .is_ok()
    );
}

/// A sensor whose `argv` names a missing file is a warning by default and a
/// failure under `--strict`; the command itself could still exit 0.
#[tokio::test(flavor = "current_thread")]
async fn warns_when_sensor_argv_path_is_missing() {
    let (_temp, root) = fake_repo_with_git();
    stub_binary(&root);
    fs::create_dir_all(root.join("scripts")).unwrap();
    fs::write(
        root.join("scripts/check-commitlint.sh"),
        "#!/usr/bin/env bash\n",
    )
    .unwrap();
    fs::write(
        root.join("do-harness.toml"),
        "[[sensors]]\nname = \"ghost\"\nargv = [\"bash\", \"scripts/does-not-exist.sh\"]\n",
    )
    .unwrap();

    assert!(
        run_with_status(&root, Format::Text, false, &repo_status(&root), None, false)
            .await
            .is_ok()
    );

    let message = run_with_status(&root, Format::Text, true, &repo_status(&root), None, false)
        .await
        .unwrap_err()
        .to_string();
    assert!(message.contains("scripts/does-not-exist.sh"), "{message}");
}

/// An installed commit-msg hook whose script is missing blocks every commit;
/// doctor must report that instead of `[OK] commit-msg hook: installed`.
#[tokio::test(flavor = "current_thread")]
async fn fails_when_commit_msg_hook_lacks_its_script() {
    let (_temp, root) = fake_repo_with_git();
    stub_binary(&root);

    let message = run_with_status(&root, Format::Text, false, &repo_status(&root), None, false)
        .await
        .unwrap_err()
        .to_string();
    assert!(message.contains("scripts/check-commitlint.sh"), "{message}");
}

/// Without the managed hook there is no dependency to check.
#[tokio::test(flavor = "current_thread")]
async fn missing_commitlint_script_is_fine_without_the_hook() {
    let (_temp, root) = fake_repo_with_git();
    stub_binary(&root);
    let mut status = repo_status(&root);
    status.commit_msg = false;
    status.ownership.commit_msg = HookOwner::Absent;

    assert!(
        run_with_status(&root, Format::Text, false, &status, None, false)
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

    let result =
        run_with_status(&root, Format::Text, false, &repo_status(&root), None, false).await;

    let message = result.unwrap_err().to_string();
    assert!(message.contains("doctor check failed"));
    assert!(message.contains("upgrade"), "{message}");
    assert!(message.contains("DO_HARNESS_BIN"), "{message}");
    // The skew is reported once, not repeated as three "unreadable" checks.
    assert!(
        !message.contains("task/event checks unreadable"),
        "{message}"
    );
    assert!(!message.contains("event chain unreadable"), "{message}");
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

    let message = run_with_status(&root, Format::Text, false, &repo_status(&root), None, false)
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

    let message = run_with_status(&root, Format::Text, false, &repo_status(&root), None, false)
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

    let message = run_with_status(&root, Format::Text, false, &repo_status(&root), None, false)
        .await
        .unwrap_err()
        .to_string();
    assert!(message.contains("tampered"), "{message}");
}

/// A wrapper in the shape `pre-commit install` writes.
const PRE_COMMIT_WRAPPER: &str = "#!/usr/bin/env bash\n\
# File generated by pre-commit: https://pre-commit.com\n\
exec pre-commit hook-impl --hook-type=pre-commit\n";

/// Writes the three hook wrappers and the configuration that declares them.
fn delegate_hooks_to_pre_commit(root: &Path) {
    for name in ["pre-commit", "pre-push", "commit-msg"] {
        let path = root.join(".git/hooks").join(name);
        fs::write(&path, PRE_COMMIT_WRAPPER).unwrap();
        crate::fs_perm::set_owner_exec(&path).unwrap();
    }
    fs::write(root.join(".pre-commit-config.yaml"), "repos: []\n").unwrap();
}

/// A sensor-free config, so strict mode reports only the hook checks.
fn quiet_config(root: &Path) {
    fs::write(root.join("do-harness.toml"), "language = \"generic\"\n").unwrap();
}

/// A status whose binary sits outside `target/`, so strict mode accepts it.
fn status_with_local_binary(root: &Path) -> hooks::HookStatus {
    let mut bin_path = root.join("bin/do-harness");
    if cfg!(windows) {
        bin_path.set_extension("exe");
    }
    fs::create_dir_all(bin_path.parent().unwrap()).unwrap();
    fs::write(&bin_path, "stub").unwrap();
    let mut status = hooks::status(&root.join(".git"), root);
    status.binary = BinSource::Path(bin_path);
    status
}

/// Delegated hooks are a healthy gate, so strict must not fail on them.
#[tokio::test(flavor = "current_thread")]
async fn strict_passes_when_hooks_are_delegated_to_pre_commit() {
    let (_temp, root) = fake_repo_with_git();
    stub_binary(&root);
    delegate_hooks_to_pre_commit(&root);
    quiet_config(&root);

    let status = status_with_local_binary(&root);

    assert_eq!(status.ownership.pre_commit, HookOwner::PreCommit);
    assert_eq!(status.ownership.commit_msg, HookOwner::PreCommit);
    assert!(
        run_with_status(&root, Format::Text, true, &status, None, false)
            .await
            .is_ok()
    );
}

/// A configuration without installed hooks is not a gate; strict must fail.
#[tokio::test(flavor = "current_thread")]
async fn strict_fails_when_pre_commit_hooks_are_not_installed() {
    let (_temp, root) = fake_repo_with_git();
    stub_binary(&root);
    fs::write(root.join(".pre-commit-config.yaml"), "repos: []\n").unwrap();
    quiet_config(&root);
    let status = status_with_local_binary(&root);
    assert_eq!(status.ownership.pre_commit, HookOwner::Absent);

    let message = run_with_status(&root, Format::Text, true, &status, None, false)
        .await
        .unwrap_err()
        .to_string();
    assert!(message.contains("neither managed"), "{message}");
}

/// A foreign hook with no manager to attribute it to is not a proven gate.
#[tokio::test(flavor = "current_thread")]
async fn strict_fails_on_an_unowned_foreign_hook() {
    let (_temp, root) = fake_repo_with_git();
    stub_binary(&root);
    fs::write(
        root.join(".git/hooks/pre-commit"),
        "#!/usr/bin/env bash\nexit 0\n",
    )
    .unwrap();
    quiet_config(&root);
    let status = status_with_local_binary(&root);
    assert_eq!(status.ownership.pre_commit, HookOwner::Foreign);

    let message = run_with_status(&root, Format::Text, true, &status, None, false)
        .await
        .unwrap_err()
        .to_string();
    assert!(message.contains("neither managed"), "{message}");
}

/// A declared configuration over empty hook files runs nothing; strict must
/// keep failing instead of trusting the declaration.
#[tokio::test(flavor = "current_thread")]
async fn strict_fails_when_declared_hooks_are_empty() {
    let (_temp, root) = fake_repo_with_git();
    stub_binary(&root);
    fs::write(root.join(".pre-commit-config.yaml"), "repos: []\n").unwrap();
    for name in ["pre-commit", "pre-push", "commit-msg"] {
        fs::write(root.join(".git/hooks").join(name), "").unwrap();
    }
    quiet_config(&root);

    let status = status_with_local_binary(&root);
    assert_eq!(status.ownership.pre_commit, HookOwner::Foreign);

    let message = run_with_status(&root, Format::Text, true, &status, None, false)
        .await
        .unwrap_err()
        .to_string();
    assert!(message.contains("neither managed"), "{message}");
}

#[test]
fn renders_delegated_and_unowned_hook_lines() {
    assert_eq!(
        hook_line("pre-commit", HookOwner::PreCommit, false, None, false, true),
        "  [OK] pre-commit hook: delegated to pre-commit"
    );
    assert_eq!(
        hook_line(
            "pre-push",
            HookOwner::HooksPath,
            false,
            Some(".githooks"),
            false,
            false
        ),
        "  [OK] pre-push hook: delegated to core.hooksPath=.githooks"
    );
    let absent = hook_line("commit-msg", HookOwner::Absent, false, None, false, true);
    assert!(
        absent.contains("[WARN]") && absent.contains("pre-commit install"),
        "{absent}"
    );
    let foreign = hook_line("pre-commit", HookOwner::Foreign, false, None, false, false);
    assert!(
        foreign.contains("[WARN]") && foreign.contains("foreign"),
        "{foreign}"
    );
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

    let message = run_with_status(&root, Format::Json, false, &repo_status(&root), None, false)
        .await
        .unwrap_err()
        .to_string();
    assert!(message.contains("tampered"), "{message}");
}
