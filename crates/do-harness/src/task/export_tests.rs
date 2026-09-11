#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;

use super::*;

#[tokio::test(flavor = "current_thread")]
async fn export_writes_task_snapshot() {
    let dir = tempfile::tempdir().unwrap();
    let conn = do_harness_db::connect_and_migrate(dir.path())
        .await
        .unwrap();
    let _ = do_harness_db::insert_task_with_event(
        &conn,
        &do_harness_db::NewTask {
            title: "slice",
            method: Some("vertical-event-slice"),
            subtask_index: 0,
            precondition: None,
            parent_id: None,
        },
    )
    .await
    .unwrap();
    drop(conn);

    let count = export_tasks(dir.path(), None, false, Format::Json)
        .await
        .unwrap();

    assert_eq!(count, 1);
    let text = fs::read_to_string(dir.path().join("plans/tasks.json")).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(parsed["tasks"][0]["title"], "slice");
    assert_eq!(parsed["tasks"][0]["status"], "pending");
    assert_eq!(parsed["tasks"][0]["method"], "vertical-event-slice");
    assert_eq!(parsed["summary"]["pending"], 1);
    assert_eq!(parsed["summary"]["done"], 0);
}

/// `import --check` passes on a fresh export and fails after database drift.
#[tokio::test(flavor = "current_thread")]
async fn import_check_detects_snapshot_drift() {
    let dir = tempfile::tempdir().unwrap();
    let task_id = {
        let conn = do_harness_db::connect_and_migrate(dir.path())
            .await
            .unwrap();
        let (id, _) = do_harness_db::insert_task_with_event(
            &conn,
            &do_harness_db::NewTask {
                title: "slice",
                method: Some("vertical-event-slice"),
                subtask_index: 0,
                precondition: None,
                parent_id: None,
            },
        )
        .await
        .unwrap();
        id
    };
    export_tasks(dir.path(), None, false, Format::Json)
        .await
        .unwrap();

    import_tasks(dir.path(), None, true).await.unwrap();

    // Drift the database after exporting, then the check must fail.
    {
        let conn = do_harness_db::connect_and_migrate(dir.path())
            .await
            .unwrap();
        do_harness_db::advance_subtask_with_event(&conn, task_id, None)
            .await
            .unwrap();
    }
    let err = import_tasks(dir.path(), None, true).await.unwrap_err();
    assert!(err.to_string().contains("drift"), "{err}");
}

#[tokio::test(flavor = "current_thread")]
async fn export_writes_empty_snapshot_without_tasks() {
    let dir = tempfile::tempdir().unwrap();
    let count = export_tasks(dir.path(), None, false, Format::Json)
        .await
        .unwrap();
    assert_eq!(count, 0);
    let text = fs::read_to_string(dir.path().join("plans/tasks.json")).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(parsed["tasks"].as_array().unwrap().len(), 0);
}

/// `TaskSnapshot` is a stability contract: it round-trips and rejects stale
/// payloads that carry unknown fields.
#[test]
fn task_snapshot_round_trips_and_rejects_unknown_fields() {
    let snapshot = TaskSnapshot {
        exported_at: 1_700_000_000,
        tasks: vec![],
        summary: TaskSummary {
            pending: 0,
            in_progress: 0,
            done: 0,
            failed: 0,
        },
    };
    let json = serde_json::to_string(&snapshot).unwrap();
    let parsed: TaskSnapshot = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.exported_at, snapshot.exported_at);
    assert!(
        serde_json::from_str::<TaskSnapshot>(r#"{"exported_at":0,"tasks":[],"extra":1}"#).is_err()
    );
}
