//! Unchanged-input reuse persistence: recorded beat digests and the
//! exact-scope latest-beat lookup.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::super::{
    LatestSensorBeat, NewBeat, SensorOutcome, latest_sensor_beat, record_verify_batch,
};
use crate::repo::NewTask;

/// A batch outcome's input digest lands on its own beat in the same
/// transaction, and outcomes without one stay NULL (never reusable).
#[tokio::test(flavor = "current_thread")]
async fn record_verify_batch_persists_input_digest_on_the_beat() {
    let dir = tempfile::tempdir().unwrap();
    let conn = crate::migrate::connect_and_migrate(dir.path())
        .await
        .unwrap();
    let outcomes = vec![
        SensorOutcome {
            beat: NewBeat {
                task_id: None,
                scope: None,
                beat_type: "sensor",
                status: "ok",
                sensor_exit_code: Some(0),
                sensor_name: Some("probe"),
                started_at: 1,
                completed_at: Some(1),
            },
            ok: true,
            message: None,
            input_digest: Some("sha256:abc"),
        },
        SensorOutcome {
            beat: NewBeat {
                task_id: None,
                scope: None,
                beat_type: "sensor",
                status: "ok",
                sensor_exit_code: Some(0),
                sensor_name: Some("plain"),
                started_at: 1,
                completed_at: Some(1),
            },
            ok: true,
            message: None,
            input_digest: None,
        },
    ];
    record_verify_batch(&conn, &outcomes).await.unwrap();

    assert_eq!(
        latest_sensor_beat(&conn, "probe", "global").await.unwrap(),
        Some(LatestSensorBeat {
            id: 1,
            status: "ok".to_owned(),
            exit_code: Some(0),
            input_digest: Some("sha256:abc".to_owned()),
        })
    );
    assert_eq!(
        latest_sensor_beat(&conn, "plain", "global").await.unwrap(),
        Some(LatestSensorBeat {
            id: 2,
            status: "ok".to_owned(),
            exit_code: Some(0),
            input_digest: None,
        })
    );
    assert_eq!(
        latest_sensor_beat(&conn, "missing", "global")
            .await
            .unwrap(),
        None
    );
}

/// A shared sensor beat builder for the `latest_sensor_beat` scope tests.
fn probe_beat<'a>(
    scope: Option<&'a str>,
    task: Option<i64>,
    status: &'a str,
    exit: Option<i32>,
) -> NewBeat<'a> {
    NewBeat {
        task_id: task,
        scope,
        beat_type: "sensor",
        status,
        sensor_exit_code: exit,
        sensor_name: Some("probe"),
        started_at: 1,
        completed_at: Some(1),
    }
}

/// The newest beat in a scope always wins: a later failure shadows an
/// earlier pass in the same scope.
#[tokio::test(flavor = "current_thread")]
async fn latest_sensor_beat_shadows_earlier_pass_in_its_scope() {
    let dir = tempfile::tempdir().unwrap();
    let conn = crate::migrate::connect_and_migrate(dir.path())
        .await
        .unwrap();

    record_verify_batch(
        &conn,
        &[
            SensorOutcome {
                beat: probe_beat(None, None, "ok", Some(0)),
                ok: true,
                message: None,
                input_digest: Some("sha256:pass"),
            },
            SensorOutcome {
                beat: probe_beat(None, None, "failed", Some(1)),
                ok: false,
                message: Some("boom"),
                input_digest: None,
            },
        ],
    )
    .await
    .unwrap();

    let latest = latest_sensor_beat(&conn, "probe", "global")
        .await
        .unwrap()
        .expect("failure beat");
    assert_eq!(latest.id, 2);
    assert_eq!(latest.status, "failed");
    assert_eq!(latest.exit_code, Some(1));
    assert_eq!(
        latest.input_digest, None,
        "a failed beat never carries a reusable digest"
    );
}

/// Branch/task/global namespaces never leak into each other: a beat recorded
/// in one scope is invisible to a lookup for another, and a scope-less task
/// beat still derives `task:<id>`.
#[tokio::test(flavor = "current_thread")]
async fn latest_sensor_beat_isolates_branch_task_and_global_scopes() {
    fn record<'a>(beat: NewBeat<'a>, digest: Option<&'a str>) -> SensorOutcome<'a> {
        let ok = beat.status == "ok";
        SensorOutcome {
            beat,
            ok,
            message: (!ok).then_some("boom"),
            input_digest: if ok { digest } else { None },
        }
    }

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

    record_verify_batch(
        &conn,
        &[
            record(probe_beat(None, None, "ok", Some(0)), Some("sha256:pass")),
            record(
                probe_beat(Some("branch:feature"), None, "ok", Some(0)),
                Some("sha256:branch"),
            ),
            record(
                probe_beat(None, Some(task_id), "ok", Some(0)),
                Some("sha256:task"),
            ),
        ],
    )
    .await
    .unwrap();

    assert_eq!(
        latest_sensor_beat(&conn, "probe", &format!("task:{task_id}"))
            .await
            .unwrap(),
        Some(LatestSensorBeat {
            id: 3,
            status: "ok".to_owned(),
            exit_code: Some(0),
            input_digest: Some("sha256:task".to_owned()),
        }),
        "the scope-less task beat derives `task:<id>`"
    );
    assert_eq!(
        latest_sensor_beat(&conn, "probe", &format!("task:{}", task_id + 1))
            .await
            .unwrap(),
        None,
        "another task namespace must not see the beat"
    );
    assert_eq!(
        latest_sensor_beat(&conn, "probe", "branch:main")
            .await
            .unwrap(),
        None,
        "a branch beat never satisfies a lookup for another branch"
    );
    assert_eq!(
        latest_sensor_beat(&conn, "probe", "branch:feature")
            .await
            .unwrap(),
        Some(LatestSensorBeat {
            id: 2,
            status: "ok".to_owned(),
            exit_code: Some(0),
            input_digest: Some("sha256:branch".to_owned()),
        }),
        "the branch scope keeps its own latest beat"
    );
    assert_eq!(
        latest_sensor_beat(&conn, "probe", "global").await.unwrap(),
        Some(LatestSensorBeat {
            id: 1,
            status: "ok".to_owned(),
            exit_code: Some(0),
            input_digest: Some("sha256:pass".to_owned()),
        }),
        "the global scope keeps its own latest beat"
    );
}
