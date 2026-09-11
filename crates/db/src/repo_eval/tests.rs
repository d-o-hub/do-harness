#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;

fn connect() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(crate::root::db_path(dir.path()).parent().unwrap()).unwrap();
    dir
}

async fn open(dir: &tempfile::TempDir) -> Connection {
    crate::migrate::connect_and_migrate(dir.path())
        .await
        .unwrap()
}

#[tokio::test(flavor = "current_thread")]
async fn eval_runs_append_and_list_in_order() {
    let dir = connect();
    let conn = open(&dir).await;
    insert_skill_eval_run(
        &conn,
        &NewSkillEvalRun {
            skill_name: "harness",
            graded: 4,
            passed: 3,
            pass_rate: Some(0.75),
        },
    )
    .await
    .unwrap();
    insert_skill_eval_run(
        &conn,
        &NewSkillEvalRun {
            skill_name: "harness",
            graded: 4,
            passed: 4,
            pass_rate: Some(1.0),
        },
    )
    .await
    .unwrap();

    let runs = list_skill_eval_runs(&conn, "harness").await.unwrap();
    assert_eq!(runs.len(), 2);
    assert_eq!(runs[0].passed, 3);
    assert_eq!(runs[1].passed, 4);
    assert!(
        list_skill_eval_runs(&conn, "other")
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test(flavor = "current_thread")]
async fn skill_bar_never_lowers() {
    let dir = connect();
    let conn = open(&dir).await;
    assert!(get_skill_bar(&conn, "harness").await.unwrap().is_none());
    assert!(raise_skill_bar(&conn, "harness", 0.9).await.unwrap());
    assert_eq!(get_skill_bar(&conn, "harness").await.unwrap(), Some(0.9));
    // A lower floor is refused; the stored bar stays at 0.95 after a raise.
    assert!(!raise_skill_bar(&conn, "harness", 0.5).await.unwrap());
    assert_eq!(get_skill_bar(&conn, "harness").await.unwrap(), Some(0.9));
    assert!(raise_skill_bar(&conn, "harness", 0.95).await.unwrap());
    assert_eq!(get_skill_bar(&conn, "harness").await.unwrap(), Some(0.95));
}

#[tokio::test(flavor = "current_thread")]
async fn grader_baseline_upserts_per_skill() {
    let dir = connect();
    let conn = open(&dir).await;
    assert!(
        get_grader_baseline(&conn, "harness")
            .await
            .unwrap()
            .is_none()
    );
    bless_grader_baseline(
        &conn,
        "harness",
        "aaa",
        "bbb",
        "approver@example.test",
        None,
    )
    .await
    .unwrap();
    bless_grader_baseline(
        &conn,
        "harness",
        "ccc",
        "ddd",
        "approver@example.test",
        None,
    )
    .await
    .unwrap();
    let baseline = get_grader_baseline(&conn, "harness")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(baseline.walkthrough_sha, "ccc");
    assert_eq!(baseline.specs_sha, "ddd");

    // Latest-wins baseline, append-only history: both blesses remain.
    let history = list_grader_blesses(&conn, "harness").await.unwrap();
    assert_eq!(history.len(), 2);
    assert_eq!(history[0].walkthrough_sha, "aaa");
    assert_eq!(history[1].walkthrough_sha, "ccc");
    assert_eq!(history[1].approver, "approver@example.test");
}

#[tokio::test(flavor = "current_thread")]
async fn max_pass_rate_tracks_history() {
    let dir = connect();
    let conn = open(&dir).await;
    assert_eq!(max_pass_rate(&conn, "none").await.unwrap(), None);
    insert_skill_eval_run(
        &conn,
        &NewSkillEvalRun {
            skill_name: "harness",
            graded: 2,
            passed: 1,
            pass_rate: Some(0.5),
        },
    )
    .await
    .unwrap();
    insert_skill_eval_run(
        &conn,
        &NewSkillEvalRun {
            skill_name: "harness",
            graded: 2,
            passed: 2,
            pass_rate: Some(1.0),
        },
    )
    .await
    .unwrap();
    assert_eq!(max_pass_rate(&conn, "harness").await.unwrap(), Some(1.0));
}

/// The SQL summary matches the per-run aggregation and honors `since`.
#[tokio::test(flavor = "current_thread")]
async fn skill_eval_summary_aggregates_history() {
    let dir = tempfile::tempdir().unwrap();
    let conn = crate::migrate::connect_and_migrate(dir.path())
        .await
        .unwrap();
    for rate in [0.5, 1.0, 0.75] {
        insert_skill_eval_run(
            &conn,
            &NewSkillEvalRun {
                skill_name: "harness",
                graded: 4,
                passed: 3,
                pass_rate: Some(rate),
            },
        )
        .await
        .unwrap();
    }

    let summary = skill_eval_summary(&conn, None).await.unwrap();
    assert_eq!(summary.len(), 1);
    assert_eq!(summary[0].runs, 3);
    assert_eq!(summary[0].best_pass_rate, Some(1.0));
    assert_eq!(summary[0].latest_pass_rate, Some(0.75));

    // A future cutoff filters every run out.
    let empty = skill_eval_summary(&conn, Some(i64::MAX)).await.unwrap();
    assert!(empty.is_empty());
}
