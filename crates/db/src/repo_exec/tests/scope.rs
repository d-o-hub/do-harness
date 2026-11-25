//! Workstream-scope isolation: beats and sensor stats stay scoped.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::super::{NewBeat, insert_beat};
use do_harness_types::{BeatScope, BeatStatus, BeatType};

#[tokio::test(flavor = "current_thread")]
async fn beats_scope_isolates_workstreams_and_filters_stats() {
    let dir = tempfile::tempdir().unwrap();
    let conn = crate::migrate::connect_and_migrate(dir.path())
        .await
        .unwrap();

    // Beat on branch:main
    insert_beat(
        &conn,
        &NewBeat {
            task_id: None,
            scope: Some(BeatScope::Branch("main".to_owned())),
            beat_type: BeatType::Sensor,
            status: BeatStatus::Ok,
            sensor_exit_code: Some(0),
            sensor_name: Some("fmt"),
            started_at: 10,
            completed_at: Some(11),
        },
    )
    .await
    .unwrap();

    // Beat on branch:feat
    insert_beat(
        &conn,
        &NewBeat {
            task_id: None,
            scope: Some(BeatScope::Branch("feat".to_owned())),
            beat_type: BeatType::Sensor,
            status: BeatStatus::Failed,
            sensor_exit_code: Some(1),
            sensor_name: Some("fmt"),
            started_at: 20,
            completed_at: Some(21),
        },
    )
    .await
    .unwrap();

    let main_beats = crate::repo_exec::list_beats_by_scope(&conn, "branch:main")
        .await
        .unwrap();
    assert_eq!(main_beats.len(), 1);
    assert_eq!(main_beats[0].scope, BeatScope::Branch("main".to_owned()));

    let feat_beats = crate::repo_exec::list_beats_by_scope(&conn, "branch:feat")
        .await
        .unwrap();
    assert_eq!(feat_beats.len(), 1);
    assert_eq!(feat_beats[0].scope, BeatScope::Branch("feat".to_owned()));

    // Unfiltered sensor stats sums both runs:
    let all_stats = crate::repo_metrics::sensor_stats(&conn, None, None)
        .await
        .unwrap();
    assert_eq!(all_stats.len(), 1);
    assert_eq!(all_stats[0].runs, 2);
    assert_eq!(all_stats[0].failures, 1);

    // Filtered by branch:main:
    let main_stats = crate::repo_metrics::sensor_stats(&conn, None, Some("branch:main"))
        .await
        .unwrap();
    assert_eq!(main_stats.len(), 1);
    assert_eq!(main_stats[0].runs, 1);
    assert_eq!(main_stats[0].failures, 0);

    // Filtered by branch:feat:
    let feat_stats = crate::repo_metrics::sensor_stats(&conn, None, Some("branch:feat"))
        .await
        .unwrap();
    assert_eq!(feat_stats.len(), 1);
    assert_eq!(feat_stats[0].runs, 1);
    assert_eq!(feat_stats[0].failures, 1);
}
