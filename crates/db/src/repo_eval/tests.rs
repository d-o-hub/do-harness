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
            mode: EvalMode::Deterministic,
            graded: 4,
            passed: 3,
            pass_rate: Some(0.75),
            without_pass_rate: None,
            skill_words: None,
            walk_secs: None,
        },
    )
    .await
    .unwrap();
    insert_skill_eval_run(
        &conn,
        &NewSkillEvalRun {
            skill_name: "harness",
            mode: EvalMode::Deterministic,
            graded: 4,
            passed: 4,
            pass_rate: Some(1.0),
            without_pass_rate: Some(0.5),
            skill_words: Some(754),
            walk_secs: Some(3.25),
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
            mode: EvalMode::Deterministic,
            graded: 2,
            passed: 1,
            pass_rate: Some(0.5),
            without_pass_rate: None,
            skill_words: None,
            walk_secs: None,
        },
    )
    .await
    .unwrap();
    insert_skill_eval_run(
        &conn,
        &NewSkillEvalRun {
            skill_name: "harness",
            mode: EvalMode::Deterministic,
            graded: 2,
            passed: 2,
            pass_rate: Some(1.0),
            without_pass_rate: None,
            skill_words: None,
            walk_secs: None,
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
                mode: EvalMode::Deterministic,
                graded: 4,
                passed: 3,
                pass_rate: Some(rate),
                without_pass_rate: None,
                skill_words: None,
                walk_secs: None,
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

/// Lift columns round-trip and dim breakdown rows attach to their run.
#[tokio::test(flavor = "current_thread")]
async fn lift_columns_and_dim_rates_roundtrip() {
    let dir = connect();
    let conn = open(&dir).await;
    let run_id = insert_skill_eval_run(
        &conn,
        &NewSkillEvalRun {
            skill_name: "harness",
            mode: EvalMode::Deterministic,
            graded: 10,
            passed: 8,
            pass_rate: Some(0.8),
            without_pass_rate: Some(0.4),
            skill_words: Some(754),
            walk_secs: Some(12.5),
        },
    )
    .await
    .unwrap();
    insert_dim_rates(
        &conn,
        run_id,
        &[
            NewSkillEvalDimRate {
                dim: "effectiveness",
                graded: 6,
                passed: 6,
                without_passed: Some(2),
            },
            NewSkillEvalDimRate {
                dim: "discoverability",
                graded: 4,
                passed: 2,
                without_passed: None,
            },
        ],
    )
    .await
    .unwrap();

    let latest = latest_eval_run(&conn, "harness").await.unwrap().unwrap();
    assert_eq!(latest.id, run_id);
    assert_eq!(latest.mode, EvalMode::Deterministic);
    assert_eq!(latest.without_pass_rate, Some(0.4));
    assert_eq!(latest.skill_words, Some(754));
    assert_eq!(latest.walk_secs, Some(12.5));
    assert!(latest_eval_run(&conn, "ghost").await.unwrap().is_none());

    let rates = dim_rates_for_run(&conn, run_id).await.unwrap();
    assert_eq!(rates.len(), 2);
    assert_eq!(rates[0].dim, "discoverability");
    assert_eq!(rates[0].without_passed, None);
    assert_eq!(rates[1].dim, "effectiveness");
    assert_eq!(rates[1].passed, 6);
    assert_eq!(rates[1].without_passed, Some(2));
    assert!(
        dim_rates_for_run(&conn, run_id + 999)
            .await
            .unwrap()
            .is_empty()
    );
}

/// The lift floor never lowers: guidance erosion below the blessed floor is
/// a gate failure, not a quiet drift.
#[tokio::test(flavor = "current_thread")]
async fn lift_floor_never_lowers() {
    let dir = connect();
    let conn = open(&dir).await;
    assert!(get_lift_floor(&conn, "harness").await.unwrap().is_none());
    assert!(raise_lift_floor(&conn, "harness", 0.15).await.unwrap());
    assert_eq!(get_lift_floor(&conn, "harness").await.unwrap(), Some(0.15));
    assert!(!raise_lift_floor(&conn, "harness", 0.10).await.unwrap());
    assert_eq!(get_lift_floor(&conn, "harness").await.unwrap(), Some(0.15));
    assert!(raise_lift_floor(&conn, "harness", 0.20).await.unwrap());
    assert_eq!(get_lift_floor(&conn, "harness").await.unwrap(), Some(0.20));
}

/// Eval mode persists per run, and an unknown stored mode degrades to the
/// default on read instead of failing the query.
#[tokio::test(flavor = "current_thread")]
async fn eval_mode_roundtrips_and_degrades_on_read() {
    let dir = connect();
    let conn = open(&dir).await;
    insert_skill_eval_run(
        &conn,
        &NewSkillEvalRun {
            skill_name: "harness",
            mode: EvalMode::Agent,
            graded: 2,
            passed: 2,
            pass_rate: Some(1.0),
            without_pass_rate: Some(0.0),
            skill_words: Some(100),
            walk_secs: Some(2.0),
        },
    )
    .await
    .unwrap();
    let latest = latest_eval_run(&conn, "harness").await.unwrap().unwrap();
    assert_eq!(latest.mode, EvalMode::Agent);

    conn.execute(
        "INSERT INTO skill_eval_runs \
         (skill_name, mode, graded, passed, pass_rate, ran_at) \
         VALUES ('harness', 'bogus', 1, 1, 1.0, 0)",
        libsql::params!(),
    )
    .await
    .unwrap();
    let degraded = latest_eval_run(&conn, "harness").await.unwrap().unwrap();
    assert_eq!(degraded.mode, EvalMode::Deterministic);
}
