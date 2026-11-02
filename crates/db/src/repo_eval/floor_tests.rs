#![allow(clippy::unwrap_used, clippy::expect_used)]

//! Floor and ratchet tests: the pass-rate bar, the Skill Lift floor, and
//! the fixture-set boundary that scopes the lift ratchet.

use super::tests::{connect, open};
use super::*;

/// The lift floor never lowers: guidance erosion below the blessed floor is
/// a gate failure, not a quiet drift.
#[tokio::test(flavor = "current_thread")]
async fn lift_floor_never_lowers() {
    let dir = connect();
    let conn = open(&dir).await;
    assert!(
        get_lift_floor(&conn, "harness", EvalMode::Deterministic)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        raise_lift_floor(&conn, "harness", EvalMode::Deterministic, 0.15)
            .await
            .unwrap()
    );
    assert_eq!(
        get_lift_floor(&conn, "harness", EvalMode::Deterministic)
            .await
            .unwrap(),
        Some(0.15)
    );
    assert!(
        !raise_lift_floor(&conn, "harness", EvalMode::Deterministic, 0.10)
            .await
            .unwrap()
    );
    assert_eq!(
        get_lift_floor(&conn, "harness", EvalMode::Deterministic)
            .await
            .unwrap(),
        Some(0.15)
    );
    assert!(
        raise_lift_floor(&conn, "harness", EvalMode::Deterministic, 0.20)
            .await
            .unwrap()
    );
    assert_eq!(
        get_lift_floor(&conn, "harness", EvalMode::Deterministic)
            .await
            .unwrap(),
        Some(0.20)
    );
}

/// `set_lift_floor` is the fixture-set boundary: unlike the ratchet it may
/// lower the floor, and it reports whether the stored value actually changed.
#[tokio::test(flavor = "current_thread")]
async fn set_lift_floor_may_lower_and_reports_changes() {
    let dir = connect();
    let conn = open(&dir).await;
    assert!(
        set_lift_floor(&conn, "harness", EvalMode::Deterministic, 0.4)
            .await
            .unwrap()
    );
    assert_eq!(
        get_lift_floor(&conn, "harness", EvalMode::Deterministic)
            .await
            .unwrap(),
        Some(0.4)
    );
    // Lowering is the whole point of the fixture-set boundary.
    assert!(
        set_lift_floor(&conn, "harness", EvalMode::Deterministic, -0.05)
            .await
            .unwrap()
    );
    assert_eq!(
        get_lift_floor(&conn, "harness", EvalMode::Deterministic)
            .await
            .unwrap(),
        Some(-0.05)
    );
    // An identical value is not a change.
    assert!(
        !set_lift_floor(&conn, "harness", EvalMode::Deterministic, -0.05)
            .await
            .unwrap()
    );
    // The ratchet still refuses to lower it afterwards.
    assert!(
        !raise_lift_floor(&conn, "harness", EvalMode::Deterministic, -0.2)
            .await
            .unwrap()
    );
    assert_eq!(
        get_lift_floor(&conn, "harness", EvalMode::Deterministic)
            .await
            .unwrap(),
        Some(-0.05)
    );
}

/// Eval mode persists per run, and an unknown stored mode is a stored-data
/// error rather than a silent default: degrading it would misreport an agent
/// run as deterministic in the lift ratchet.
#[tokio::test(flavor = "current_thread")]
async fn eval_mode_roundtrips_and_rejects_unknown_stored_mode() {
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
    let err = latest_eval_run(&conn, "harness").await.unwrap_err();
    assert!(
        matches!(&err, DbError::InvalidEvalMode(raw) if raw == "bogus"),
        "{err}"
    );
}

/// Floors are mode-scoped: an agent floor never governs deterministic runs
/// and vice versa.
#[tokio::test(flavor = "current_thread")]
async fn floors_are_scoped_per_eval_mode() {
    let dir = connect();
    let conn = open(&dir).await;
    assert!(
        raise_skill_bar(&conn, "harness", EvalMode::Deterministic, 0.9)
            .await
            .unwrap()
    );
    assert!(
        raise_lift_floor(&conn, "harness", EvalMode::Agent, 0.4)
            .await
            .unwrap()
    );

    assert_eq!(
        get_skill_bar(&conn, "harness", EvalMode::Deterministic)
            .await
            .unwrap(),
        Some(0.9)
    );
    assert_eq!(
        get_skill_bar(&conn, "harness", EvalMode::Agent)
            .await
            .unwrap(),
        None
    );
    assert_eq!(
        get_lift_floor(&conn, "harness", EvalMode::Agent)
            .await
            .unwrap(),
        Some(0.4)
    );
    assert_eq!(
        get_lift_floor(&conn, "harness", EvalMode::Deterministic)
            .await
            .unwrap(),
        None
    );
}
