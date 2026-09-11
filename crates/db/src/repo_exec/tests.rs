#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;
use crate::repo::NewTask;

#[tokio::test(flavor = "current_thread")]
async fn insert_beat_roundtrips_and_filters_by_task() {
    let dir = tempfile::tempdir().unwrap();
    let conn = crate::migrate::connect_and_migrate(dir.path())
        .await
        .unwrap();
    let task_id = crate::repo::insert_task(
        &conn,
        &NewTask {
            title: "slice",
            method: Some("vertical-event-slice"),
            subtask_index: 0,
            precondition: None,
            parent_id: None,
        },
    )
    .await
    .unwrap();
    insert_beat(
        &conn,
        &NewBeat {
            task_id: Some(task_id),
            beat_type: "sensor",
            status: "failed",
            sensor_exit_code: Some(1),
            sensor_name: Some("check"),
            started_at: 1,
            completed_at: Some(2),
        },
    )
    .await
    .unwrap();

    let beats = list_beats(&conn, Some(task_id)).await.unwrap();
    assert_eq!(beats.len(), 1);
    assert_eq!(beats[0].beat_type, "sensor");
    assert_eq!(beats[0].status, "failed");
    assert_eq!(beats[0].sensor_exit_code, Some(1));
    assert_eq!(beats[0].started_at, 1);
    assert_eq!(list_beats(&conn, None).await.unwrap().len(), 1);
    assert!(
        list_beats(&conn, Some(task_id + 1))
            .await
            .unwrap()
            .is_empty()
    );
}

/// Foreign keys are enforced: a beat referencing a missing task is
/// rejected instead of silently orphaned.
#[tokio::test(flavor = "current_thread")]
async fn insert_beat_rejects_missing_task_fk() {
    let dir = tempfile::tempdir().unwrap();
    let conn = crate::migrate::connect_and_migrate(dir.path())
        .await
        .unwrap();
    let result = insert_beat(
        &conn,
        &NewBeat {
            task_id: Some(9999),
            beat_type: "sensor",
            status: "ok",
            sensor_exit_code: Some(0),
            sensor_name: Some("check"),
            started_at: 1,
            completed_at: Some(2),
        },
    )
    .await;
    assert!(result.is_err(), "FK violation must surface as an error");
}

/// Workspace-global strikes (NULL `task_id`) are unique per signature: a
/// raw duplicate insert violates the partial unique index.
#[tokio::test(flavor = "current_thread")]
async fn duplicate_global_signature_insert_is_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let conn = crate::migrate::connect_and_migrate(dir.path())
        .await
        .unwrap();
    bump_error_signature(&conn, "sensor:clippy", None, Some("m1"))
        .await
        .unwrap();
    let dupe = conn
        .execute(
            "INSERT INTO error_signatures (signature, task_id, attempt_count, message, \
             created_at) VALUES ('sensor:clippy', NULL, 1, NULL, 0)",
            Params::None,
        )
        .await;
    assert!(dupe.is_err(), "duplicate global strike must be rejected");
}

#[tokio::test(flavor = "current_thread")]
async fn record_sensor_outcome_is_atomic_beat_plus_strike() {
    let dir = tempfile::tempdir().unwrap();
    let conn = crate::migrate::connect_and_migrate(dir.path())
        .await
        .unwrap();

    let beat = |status: &'static str| NewBeat {
        task_id: None,
        beat_type: "sensor",
        status,
        sensor_exit_code: Some(0),
        sensor_name: Some("atomic"),
        started_at: 1,
        completed_at: Some(2),
    };
    record_sensor_outcome(&conn, &beat("failed"), false, Some("boom"))
        .await
        .unwrap();
    record_sensor_outcome(&conn, &beat("failed"), false, Some("boom2"))
        .await
        .unwrap();
    assert_eq!(
        get_error_signature(&conn, "sensor:atomic", None)
            .await
            .unwrap()
            .unwrap()
            .attempt_count,
        2
    );
    record_sensor_outcome(&conn, &beat("ok"), true, None)
        .await
        .unwrap();
    assert!(
        get_error_signature(&conn, "sensor:atomic", None)
            .await
            .unwrap()
            .is_none(),
        "passing outcome must reset the strike inside the same transaction"
    );
    assert_eq!(list_beats(&conn, None).await.unwrap().len(), 3);
}

#[tokio::test(flavor = "current_thread")]
async fn bump_error_signature_starts_at_one_and_increments() {
    let dir = tempfile::tempdir().unwrap();
    let conn = crate::migrate::connect_and_migrate(dir.path())
        .await
        .unwrap();

    assert_eq!(
        bump_error_signature(&conn, "sensor:clippy", None, Some("m1"))
            .await
            .unwrap(),
        1
    );
    assert_eq!(
        bump_error_signature(&conn, "sensor:clippy", None, Some("m2"))
            .await
            .unwrap(),
        2
    );
    assert_eq!(
        bump_error_signature(&conn, "sensor:clippy", None, None)
            .await
            .unwrap(),
        3
    );

    let sig = get_error_signature(&conn, "sensor:clippy", None)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(sig.attempt_count, 3);
    assert_eq!(sig.message.as_deref(), Some("m2"));
}

/// A failing outcome in the middle of a batch rolls back every earlier
/// beat: verify runs either persist completely or not at all.
#[tokio::test(flavor = "current_thread")]
async fn record_verify_batch_rolls_back_everything_on_failure() {
    let dir = tempfile::tempdir().unwrap();
    let conn = crate::migrate::connect_and_migrate(dir.path())
        .await
        .unwrap();
    let outcomes = vec![
        SensorOutcome {
            beat: NewBeat {
                task_id: None,
                beat_type: "sensor",
                status: "ok",
                sensor_exit_code: Some(0),
                sensor_name: Some("check"),
                started_at: 1,
                completed_at: Some(1),
            },
            ok: true,
            message: None,
        },
        SensorOutcome {
            // FK violation: task 9999 does not exist.
            beat: NewBeat {
                task_id: Some(9999),
                beat_type: "sensor",
                status: "failed",
                sensor_exit_code: Some(1),
                sensor_name: Some("test"),
                started_at: 1,
                completed_at: Some(1),
            },
            ok: false,
            message: Some("boom"),
        },
    ];

    assert!(record_verify_batch(&conn, &outcomes).await.is_err());
    assert!(
        list_beats(&conn, None).await.unwrap().is_empty(),
        "first beat must be rolled back with the failed batch"
    );
    assert!(
        get_error_signature(&conn, "sensor:test", Some(9999))
            .await
            .unwrap()
            .is_none(),
        "signature bump must be rolled back too"
    );
}

/// Pruning keeps the newest beats per task even when they are older than
/// the cutoff.
#[tokio::test(flavor = "current_thread")]
async fn prune_beats_keeps_newest_per_task() {
    let dir = tempfile::tempdir().unwrap();
    let conn = crate::migrate::connect_and_migrate(dir.path())
        .await
        .unwrap();

    for index in 0..5 {
        insert_beat(
            &conn,
            &NewBeat {
                task_id: None,
                beat_type: "sensor",
                status: "ok",
                sensor_exit_code: Some(0),
                sensor_name: Some("check"),
                started_at: i64::from(index),
                completed_at: Some(i64::from(index)),
            },
        )
        .await
        .unwrap();
    }
    // All beats are older than the cutoff; the newest two must survive.
    let deleted = prune_beats(&conn, 100, 2).await.unwrap();
    assert_eq!(deleted, 3);
    let remaining = list_beats(&conn, None).await.unwrap();
    assert_eq!(remaining.len(), 2);
    assert_eq!(remaining[0].id, 4);
    assert_eq!(remaining[1].id, 5);
}
