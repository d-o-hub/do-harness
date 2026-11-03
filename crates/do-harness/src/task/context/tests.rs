#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;

use super::super::tests::{insert_ok_beat, write_catalog};
use super::*;
use do_harness_types::{BeatScope, BeatStatus, BeatType};

/// Fresh catalog, git repo, and one task on the first sensor-gated subtask.
async fn fixture() -> (tempfile::TempDir, i64) {
    let dir = tempfile::tempdir().unwrap();
    write_catalog(dir.path());
    let (id, _event) = crate::task::add_task(
        dir.path(),
        "implement the projection",
        Some("vertical-event-slice"),
        None,
        None,
    )
    .await
    .unwrap();
    (dir, id)
}

#[tokio::test(flavor = "current_thread")]
async fn missing_beat_derives_verify_action() {
    let (dir, id) = fixture().await;
    let doc = build(dir.path(), id, DEFAULT_MAX_BYTES).await.unwrap();
    let step = doc.step.as_ref().unwrap();
    assert_eq!(step.state, "current");
    assert_eq!(step.name.as_deref(), Some("write-acceptance-test"));
    assert_eq!(step.sensor.as_deref(), Some("test"));
    let gating = doc.gating.as_ref().unwrap();
    assert_eq!(gating.required_sensors, vec!["test"]);
    assert_eq!(gating.state, "missing");
    assert_eq!(gating.reason, "no_beat");
    assert_eq!(doc.next_action.kind, "verify");
    assert_eq!(doc.next_action.reason, "evidence_missing");
    assert_eq!(
        doc.next_action.command.as_deref(),
        Some(format!("do-harness verify --only test --record --task {id}").as_str())
    );
    assert!(!doc.acceptance_criteria.available);
    assert_eq!(doc.acceptance_criteria.reason, "not_recorded");
    assert!(
        doc.unknowns
            .iter()
            .any(|u| u.section == "acceptance_criteria" && u.reason == "not_recorded")
    );
    assert_eq!(doc.workspace.scope, "workspace");
}

#[tokio::test(flavor = "current_thread")]
async fn fresh_beat_derives_advance_action() {
    let (dir, id) = fixture().await;
    insert_ok_beat(dir.path(), id, "test").await;
    let doc = build(dir.path(), id, DEFAULT_MAX_BYTES).await.unwrap();
    let gating = doc.gating.as_ref().unwrap();
    assert_eq!(gating.state, "green");
    assert_eq!(gating.reason, "current");
    assert_eq!(doc.next_action.kind, "advance");
    assert_eq!(doc.next_action.reason, "sensor_passing");
}

#[tokio::test(flavor = "current_thread")]
async fn source_edit_after_green_is_stale() {
    let dir = tempfile::tempdir().unwrap();
    write_catalog(dir.path());
    fs::create_dir_all(dir.path().join("src")).unwrap();
    fs::write(dir.path().join("src/lib.rs"), "pub fn one() {}\n").unwrap();
    let (id, _event) = crate::task::add_task(
        dir.path(),
        "edit source",
        Some("vertical-event-slice"),
        None,
        None,
    )
    .await
    .unwrap();
    insert_ok_beat(dir.path(), id, "test").await;
    assert_eq!(
        build(dir.path(), id, DEFAULT_MAX_BYTES)
            .await
            .unwrap()
            .gating
            .unwrap()
            .state,
        "green"
    );

    fs::write(dir.path().join("src/lib.rs"), "pub fn two() {}\n").unwrap();
    let doc = build(dir.path(), id, DEFAULT_MAX_BYTES).await.unwrap();
    let gating = doc.gating.unwrap();
    assert_eq!(gating.state, "stale");
    assert_eq!(gating.reason, "inputs_changed");
    assert_eq!(doc.next_action.reason, "evidence_stale");
}

#[tokio::test(flavor = "current_thread")]
async fn task_state_change_is_visible_between_reads() {
    let (dir, id) = fixture().await;
    let first = build(dir.path(), id, DEFAULT_MAX_BYTES).await.unwrap();
    assert_eq!(first.step.as_ref().unwrap().index, 0);

    insert_ok_beat(dir.path(), id, "test").await;
    crate::task::advance_task(dir.path(), id).await.unwrap();

    let second = build(dir.path(), id, DEFAULT_MAX_BYTES).await.unwrap();
    let step = second.step.as_ref().unwrap();
    assert_eq!(step.index, 1);
    assert_eq!(step.name.as_deref(), Some("implement-slice"));
}

#[tokio::test(flavor = "current_thread")]
async fn reading_does_not_alter_database_or_write_evidence() {
    let (dir, id) = fixture().await;
    fs::create_dir_all(dir.path().join(".agents/skills/demo")).unwrap();
    fs::write(
        dir.path().join(".agents/skills/demo/SKILL.md"),
        "---\nname: demo\ndescription: demo skill\n---\nbody\n",
    )
    .unwrap();
    let db_path = do_harness_db::db_path(dir.path());
    let before = fs::read(&db_path).unwrap();
    assert!(!dir.path().join(".do-harness/evidence.json").exists());

    let mut doc = build(dir.path(), id, DEFAULT_MAX_BYTES).await.unwrap();
    fit(&mut doc, DEFAULT_MAX_BYTES, Format::Json).unwrap();
    fit(&mut doc, DEFAULT_MAX_BYTES, Format::Text).unwrap();

    assert_eq!(fs::read(&db_path).unwrap(), before);
    assert!(!dir.path().join(".do-harness/evidence.json").exists());
    assert!(
        !dir.path()
            .join(".do-harness/evidence.verification.json")
            .exists()
    );
    assert!(
        !dir.path().join(".do-harness/cache/skills-v1.json").exists(),
        "reading context must not write the skill cache"
    );
    assert!(doc.skills.items.iter().any(|s| s.name == "demo"));
}

#[tokio::test(flavor = "current_thread")]
async fn branch_and_global_observations_do_not_satisfy_the_task() {
    let (dir, id) = fixture().await;
    let conn = do_harness_db::connect_and_migrate(dir.path())
        .await
        .unwrap();
    let now = do_harness_db::unix_now();
    do_harness_db::record_verify_batch(
        &conn,
        &[do_harness_db::SensorOutcome {
            beat: do_harness_db::NewBeat {
                task_id: None,
                scope: Some(BeatScope::Branch("main".to_owned())),
                beat_type: BeatType::Sensor,
                status: BeatStatus::Ok,
                sensor_exit_code: Some(0),
                sensor_name: Some("test"),
                started_at: now,
                completed_at: Some(now),
            },
            ok: true,
            message: None,
            input_digest: None,
        }],
    )
    .await
    .unwrap();
    do_harness_db::bump_error_signature(&conn, "sensor:test", "branch:main", Some("branch"))
        .await
        .unwrap();
    do_harness_db::bump_error_signature(&conn, "sensor:test", "global", Some("global"))
        .await
        .unwrap();
    drop(conn);

    let doc = build(dir.path(), id, DEFAULT_MAX_BYTES).await.unwrap();
    assert_eq!(doc.gating.as_ref().unwrap().state, "missing");
    assert!(doc.failures.items.is_empty());
    assert_eq!(doc.failures.omitted, 0);
}

#[tokio::test(flavor = "current_thread")]
async fn task_scoped_failures_and_traces_are_reported() {
    let (dir, id) = fixture().await;
    let conn = do_harness_db::connect_and_migrate(dir.path())
        .await
        .unwrap();
    do_harness_db::bump_error_signature(&conn, "sensor:test", &format!("task:{id}"), Some("boom"))
        .await
        .unwrap();
    do_harness_db::insert_trace(
        &conn,
        &do_harness_db::NewTrace {
            task_id: Some(id),
            session_id: "s",
            command: Some("cargo test"),
            error_diff: Some("assertion failed\nwith newline"),
            resolution_steps: None,
        },
    )
    .await
    .unwrap();
    drop(conn);

    let doc = build(dir.path(), id, DEFAULT_MAX_BYTES).await.unwrap();
    assert_eq!(doc.failures.items.len(), 1);
    assert_eq!(doc.failures.items[0].signature, "sensor:test");
    assert_eq!(doc.failures.items[0].message.as_deref(), Some("boom"));
    assert_eq!(doc.traces.items.len(), 1);
    assert_eq!(doc.traces.items[0].command.as_deref(), Some("cargo test"));
    // Stored newlines cannot reshape the text view.
    let rendered = fit(&mut doc.clone(), DEFAULT_MAX_BYTES, Format::Text).unwrap();
    assert!(rendered.contains("error_diff: assertion failed with newline"));
}

#[tokio::test(flavor = "current_thread")]
async fn byte_budget_shrinks_sections_but_keeps_identity_and_next_action() {
    let (dir, id) = fixture().await;
    let conn = do_harness_db::connect_and_migrate(dir.path())
        .await
        .unwrap();
    for n in 0..12 {
        let command = format!("command-{n}");
        let diff = "x".repeat(200);
        do_harness_db::insert_trace(
            &conn,
            &do_harness_db::NewTrace {
                task_id: Some(id),
                session_id: "s",
                command: Some(&command),
                error_diff: Some(&diff),
                resolution_steps: None,
            },
        )
        .await
        .unwrap();
    }
    drop(conn);

    let mut doc = build(dir.path(), id, 400).await.unwrap();
    let rendered = fit(&mut doc, 400, Format::Text).unwrap();
    assert!(rendered.contains(&format!("task {id}:")));
    assert!(rendered.contains("next:"));
    assert!(doc.traces.truncated);
    assert!(doc.traces.omitted > 0);
    assert!(doc.traces.items.len() < 10);

    let mut roomy = build(dir.path(), id, 100_000).await.unwrap();
    let full = fit(&mut roomy, 100_000, Format::Text).unwrap();
    assert!(full.len() <= 100_000);
}

#[tokio::test(flavor = "current_thread")]
async fn json_is_versioned_and_survives_a_small_budget() {
    let (dir, id) = fixture().await;
    let mut doc = build(dir.path(), id, 500).await.unwrap();
    let rendered = fit(&mut doc, 500, Format::Json).unwrap();
    let value: serde_json::Value = serde_json::from_str(&rendered).unwrap();
    assert_eq!(value["schema_version"], CONTEXT_SCHEMA_VERSION);
    assert_eq!(value["task"]["id"], id);
    assert_eq!(value["acceptance_criteria"]["available"], false);
    assert_eq!(value["next_action"]["kind"], "verify");
    assert_eq!(value["workspace"]["scope"], "workspace");
}

#[tokio::test(flavor = "current_thread")]
async fn missing_database_is_a_diagnostic_and_is_not_created() {
    let dir = tempfile::tempdir().unwrap();
    let doc = build(dir.path(), 1, DEFAULT_MAX_BYTES).await.unwrap();
    assert_eq!(doc.database.state, "missing");
    assert_eq!(doc.database.reason, "database_missing");
    assert_eq!(doc.next_action.kind, "unavailable");
    assert!(doc.task.is_none());
    assert!(!do_harness_db::db_path(dir.path()).exists());
    assert!(
        doc.unknowns
            .iter()
            .any(|u| u.section == "task" && u.reason == "database_missing")
    );
}

#[tokio::test(flavor = "current_thread")]
async fn unknown_method_is_explicit() {
    let dir = tempfile::tempdir().unwrap();
    write_catalog(dir.path());
    let conn = do_harness_db::connect_and_migrate(dir.path())
        .await
        .unwrap();
    let (id, _event) = do_harness_db::insert_task_with_event(
        &conn,
        &do_harness_db::NewTask {
            title: "dangling",
            method: Some("ghost"),
            subtask_index: 0,
            precondition: None,
            parent_id: None,
        },
    )
    .await
    .unwrap();
    drop(conn);

    let doc = build(dir.path(), id, DEFAULT_MAX_BYTES).await.unwrap();
    assert_eq!(doc.step.as_ref().unwrap().state, "method_missing");
    assert_eq!(doc.next_action.kind, "fix_method");
    assert_eq!(doc.gating.as_ref().unwrap().state, "unavailable");
}

#[tokio::test(flavor = "current_thread")]
async fn missing_method_and_invalid_index_are_explicit() {
    let dir = tempfile::tempdir().unwrap();
    write_catalog(dir.path());
    let (plain, _event) = crate::task::add_task(dir.path(), "plain", None, None, None)
        .await
        .unwrap();
    let doc = build(dir.path(), plain, DEFAULT_MAX_BYTES).await.unwrap();
    assert_eq!(doc.step.as_ref().unwrap().state, "no_method");
    assert_eq!(doc.next_action.kind, "configure_method");

    let conn = do_harness_db::connect_and_migrate(dir.path())
        .await
        .unwrap();
    let (bad, _event) = do_harness_db::insert_task_with_event(
        &conn,
        &do_harness_db::NewTask {
            title: "bad index",
            method: Some("vertical-event-slice"),
            subtask_index: -1,
            precondition: None,
            parent_id: None,
        },
    )
    .await
    .unwrap();
    drop(conn);
    let doc = build(dir.path(), bad, DEFAULT_MAX_BYTES).await.unwrap();
    assert_eq!(doc.step.as_ref().unwrap().state, "invalid_index");
    assert_eq!(doc.next_action.kind, "fix_state");
}

#[tokio::test(flavor = "current_thread")]
async fn past_all_subtasks_derives_done_action() {
    let dir = tempfile::tempdir().unwrap();
    write_catalog(dir.path());
    let conn = do_harness_db::connect_and_migrate(dir.path())
        .await
        .unwrap();
    let (id, _event) = do_harness_db::insert_task_with_event(
        &conn,
        &do_harness_db::NewTask {
            title: "past gates",
            method: Some("mini"),
            subtask_index: 2,
            precondition: None,
            parent_id: None,
        },
    )
    .await
    .unwrap();
    drop(conn);
    let doc = build(dir.path(), id, DEFAULT_MAX_BYTES).await.unwrap();
    assert_eq!(doc.step.as_ref().unwrap().state, "complete");
    // Past every subtask but without the required passing beats, `task done`
    // would refuse, so the next action must be the verify run.
    assert_eq!(doc.next_action.kind, "verify");
    assert_eq!(doc.next_action.reason, "evidence_missing");

    insert_ok_beat(dir.path(), id, "test").await;
    let doc = build(dir.path(), id, DEFAULT_MAX_BYTES).await.unwrap();
    assert_eq!(doc.next_action.kind, "done");
    assert_eq!(doc.gating.as_ref().unwrap().state, "green");
}

#[tokio::test(flavor = "current_thread")]
async fn repeated_reads_are_stable() {
    let (dir, id) = fixture().await;
    let mut first = build(dir.path(), id, DEFAULT_MAX_BYTES).await.unwrap();
    let mut second = build(dir.path(), id, DEFAULT_MAX_BYTES).await.unwrap();
    assert_eq!(
        fit(&mut first, DEFAULT_MAX_BYTES, Format::Json).unwrap(),
        fit(&mut second, DEFAULT_MAX_BYTES, Format::Json).unwrap()
    );
}

#[tokio::test(flavor = "current_thread")]
async fn unknown_task_id_is_an_error() {
    let (dir, id) = fixture().await;
    let err = build(dir.path(), id + 100, DEFAULT_MAX_BYTES)
        .await
        .unwrap_err();
    assert!(err.to_string().contains("not found"), "unexpected: {err}");
}
