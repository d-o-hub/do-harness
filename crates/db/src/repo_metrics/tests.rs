#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;
use crate::repo_exec::{NewBeat, insert_beat};
use do_harness_types::{BeatScope, BeatStatus, BeatType};

/// Seeds one beat. Every test below cares about what the aggregate sees rather
/// than about the row itself.
async fn beat(
    conn: &Connection,
    beat_type: BeatType,
    status: BeatStatus,
    sensor_name: Option<&str>,
    scope: Option<BeatScope>,
    started_at: i64,
) {
    insert_beat(
        conn,
        &NewBeat {
            task_id: None,
            scope,
            beat_type,
            status,
            sensor_exit_code: None,
            sensor_name,
            started_at,
            completed_at: Some(started_at + 1),
            duration_ms: None,
        },
    )
    .await
    .unwrap();
}

/// Two `check` beats in `branch:main` (one ok, one failed), an older `check`
/// beat in `global`, one `test` beat, and a non-sensor beat that carries a
/// sensor name.
async fn seed_history(conn: &Connection) {
    beat(
        conn,
        BeatType::Sensor,
        BeatStatus::Ok,
        Some("check"),
        Some(BeatScope::Global),
        50,
    )
    .await;
    beat(
        conn,
        BeatType::Sensor,
        BeatStatus::Ok,
        Some("check"),
        Some(BeatScope::Branch("main".to_owned())),
        100,
    )
    .await;
    beat(
        conn,
        BeatType::Sensor,
        BeatStatus::Failed,
        Some("check"),
        Some(BeatScope::Branch("main".to_owned())),
        200,
    )
    .await;
    beat(
        conn,
        BeatType::Sensor,
        BeatStatus::Ok,
        Some("test"),
        Some(BeatScope::Branch("main".to_owned())),
        300,
    )
    .await;
    beat(
        conn,
        BeatType::Step,
        BeatStatus::Failed,
        Some("check"),
        Some(BeatScope::Global),
        400,
    )
    .await;
}

fn stat(name: &str, runs: i64, failures: i64) -> SensorStat {
    SensorStat {
        name: name.to_owned(),
        runs,
        failures,
    }
}

async fn open() -> (tempfile::TempDir, Connection) {
    let dir = tempfile::tempdir().unwrap();
    let conn = crate::migrate::connect_and_migrate(dir.path())
        .await
        .unwrap();
    (dir, conn)
}

/// The per-sensor aggregate groups by sensor name, counts non-`ok` outcomes as
/// failures, and ignores beats that are not sensor beats.
#[tokio::test(flavor = "current_thread")]
async fn sensor_stats_groups_counts_and_ignores_non_sensor_beats() {
    let (_dir, conn) = open().await;
    seed_history(&conn).await;

    assert_eq!(
        sensor_stats(&conn, None, None).await.unwrap(),
        vec![stat("check", 3, 1), stat("test", 1, 0)],
        "grouped by sensor name and ordered by it; the `task` beat is not a sensor run"
    );

    // The query also guards `sensor_name IS NOT NULL`, and the schema makes that
    // case unrepresentable: the CHECK `beat_type != 'sensor' OR sensor_name IS
    // NOT NULL` rejects a nameless sensor beat outright.
    let rejected = insert_beat(
        &conn,
        &NewBeat {
            task_id: None,
            scope: Some(BeatScope::Global),
            beat_type: BeatType::Sensor,
            status: BeatStatus::Failed,
            sensor_exit_code: None,
            sensor_name: None,
            started_at: 600,
            completed_at: Some(601),
            duration_ms: None,
        },
    )
    .await;
    assert!(
        rejected.is_err(),
        "a sensor beat without a sensor name must be rejected by the schema"
    );
}

/// Both filters narrow the aggregate: `since` drops older beats, and `scope`
/// selects one workstream instead of silently widening to every beat.
#[tokio::test(flavor = "current_thread")]
async fn sensor_stats_honours_since_and_scope() {
    let (_dir, conn) = open().await;
    seed_history(&conn).await;

    assert_eq!(
        sensor_stats(&conn, Some(100), None).await.unwrap(),
        vec![stat("check", 2, 1), stat("test", 1, 0)],
        "`since` keeps beats with started_at >= the bound"
    );
    assert_eq!(
        sensor_stats(&conn, None, Some("branch:main"))
            .await
            .unwrap(),
        vec![stat("check", 2, 1), stat("test", 1, 0)],
        "`scope` keeps one workstream only"
    );
    assert!(
        sensor_stats(&conn, None, Some("task:99"))
            .await
            .unwrap()
            .is_empty(),
        "an unused scope reports nothing rather than every beat"
    );
}

/// The existence probe answers exactly "has any beat ever succeeded": false on
/// an empty table and after failures, true for a success of any beat type.
#[tokio::test(flavor = "current_thread")]
async fn has_ok_beat_reflects_the_status_column() {
    let (_dir, conn) = open().await;
    assert!(!has_ok_beat(&conn).await.unwrap(), "empty table");

    beat(
        &conn,
        BeatType::Sensor,
        BeatStatus::Failed,
        Some("check"),
        Some(BeatScope::Global),
        1,
    )
    .await;
    assert!(!has_ok_beat(&conn).await.unwrap(), "failures only");

    beat(
        &conn,
        BeatType::Step,
        BeatStatus::Ok,
        None,
        Some(BeatScope::Global),
        2,
    )
    .await;
    assert!(
        has_ok_beat(&conn).await.unwrap(),
        "any beat with status 'ok' counts, not only sensor beats"
    );
}

/// The export prefers the duration the runner measured and `verify --record`
/// persisted; legacy rows without a measurement fall back to the timestamp
/// difference; a row stamped only with the recording time reports an unknown
/// duration; and a measured zero stays a zero, distinguishable from missing
/// data.
#[tokio::test(flavor = "current_thread")]
async fn sensor_executions_prefers_the_measured_duration() {
    let (_dir, conn) = open().await;
    for (name, started_at, completed_at, duration_ms) in [
        ("measured", 500, Some(500), Some(1250)),
        ("legacy", 100, Some(102), None),
        ("placeholder", 200, Some(200), None),
        ("zero", 300, Some(300), Some(0)),
    ] {
        insert_beat(
            &conn,
            &NewBeat {
                task_id: None,
                scope: Some(BeatScope::Global),
                beat_type: BeatType::Sensor,
                status: BeatStatus::Ok,
                sensor_exit_code: Some(0),
                sensor_name: Some(name),
                started_at,
                completed_at,
                duration_ms,
            },
        )
        .await
        .unwrap();
    }

    let executions = sensor_executions(&conn, None, None, None, None)
        .await
        .unwrap();
    let duration = |sensor: &str| {
        executions
            .iter()
            .find(|row| row.sensor == sensor)
            .unwrap()
            .duration_ms
    };
    assert_eq!(duration("measured"), Some(1250), "the measurement wins");
    assert_eq!(
        duration("legacy"),
        Some(2000),
        "no measurement: derived from the timestamps"
    );
    assert_eq!(
        duration("placeholder"),
        None,
        "identical stamps without a measurement are unknown, not 0 ms"
    );
    assert_eq!(
        duration("zero"),
        Some(0),
        "a measured zero is a real measurement"
    );
}

/// The export keeps every execution in the window, oldest first, and resolves
/// the duration for rows without a measurement from the two timestamps;
/// filters compose instead of widening.
#[tokio::test(flavor = "current_thread")]
async fn sensor_executions_exports_the_window_oldest_first() {
    let (_dir, conn) = open().await;
    seed_history(&conn).await;
    insert_beat(
        &conn,
        &NewBeat {
            task_id: None,
            scope: Some(BeatScope::Branch("main".to_owned())),
            beat_type: BeatType::Sensor,
            status: BeatStatus::Failed,
            sensor_exit_code: Some(2),
            sensor_name: Some("check"),
            started_at: 250,
            completed_at: Some(253),
            duration_ms: None,
        },
    )
    .await
    .unwrap();

    let all = sensor_executions(&conn, None, None, None, None)
        .await
        .unwrap();
    let names: Vec<&str> = all.iter().map(|row| row.sensor.as_str()).collect();
    assert_eq!(names, vec!["check", "check", "check", "check", "test"]);
    let times: Vec<i64> = all.iter().map(|row| row.started_at).collect();
    assert_eq!(times, vec![50, 100, 200, 250, 300]);
    assert_eq!(
        all.iter().map(|row| row.duration_ms).collect::<Vec<_>>(),
        vec![Some(1000), Some(1000), Some(1000), Some(3000), Some(1000)]
    );
    assert_eq!(all[3].exit_code, Some(2));
    assert_eq!(all[3].status, "failed");
    assert_eq!(all[0].scope, "global");

    // Window + scope + sensor compose: only the two in `branch:main` inside
    // 100..=200 remain, and the older `global` beat stays out.
    let windowed = sensor_executions(
        &conn,
        Some(100),
        Some(200),
        Some("branch:main"),
        Some("check"),
    )
    .await
    .unwrap();
    assert_eq!(
        windowed
            .iter()
            .map(|row| row.started_at)
            .collect::<Vec<_>>(),
        vec![100, 200]
    );

    assert!(
        sensor_executions(&conn, None, None, Some("task:99"), None)
            .await
            .unwrap()
            .is_empty(),
        "an unused scope reports nothing"
    );
}
