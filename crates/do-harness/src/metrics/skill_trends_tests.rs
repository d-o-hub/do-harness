//! Tests for the skill-trends aggregation behind `metrics`.
//!
//! `collect_skill_trends` is where the reported numbers are joined: the latest
//! pass rate (from `skill_evals`), the best-ever rate and run count (from the
//! run history), the blessed Skill-Lift floor and bar floor (from the floors
//! tables), Skill Lift itself (with minus without on the newest run), and the
//! per-dimension rates with and without the skill. Every one of those numbers
//! was printed and never asserted.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use do_harness_db::{
    Connection, NewSkillEval, NewSkillEvalDimRate, NewSkillEvalRun, connect_and_migrate,
    get_lift_floor, get_skill_bar, insert_skill_eval_run, raise_lift_floor, raise_skill_bar,
    record_eval_run,
};
use do_harness_types::EvalMode;

use super::collect_skill_trends;

/// Opens a migrated database in a temporary root.
async fn open() -> (tempfile::TempDir, Connection) {
    let dir = tempfile::tempdir().unwrap();
    let conn = connect_and_migrate(dir.path()).await.unwrap();
    (dir, conn)
}

/// Records one run of `skill`, with the latest-skill row that accompanies it.
async fn record(
    conn: &Connection,
    skill: &str,
    run: NewSkillEvalRun<'_>,
    dims: &[NewSkillEvalDimRate<'_>],
    latest_rate: f64,
) {
    record_eval_run(
        conn,
        &NewSkillEval {
            skill_name: skill,
            prompt: None,
            expected_outcome: None,
            pass_rate: Some(latest_rate),
        },
        &run,
        dims,
    )
    .await
    .unwrap();
}

/// The main join: two runs, blessed floors, and per-dimension rates with and
/// without the skill.
#[tokio::test(flavor = "current_thread")]
async fn trends_join_runs_floors_and_dimension_rates() {
    let (_dir, conn) = open().await;

    // Older run: no baseline measured, so it contributes no lift.
    record(
        &conn,
        "harness",
        NewSkillEvalRun {
            skill_name: "harness",
            mode: EvalMode::Deterministic,
            graded: 4,
            passed: 3,
            pass_rate: Some(0.75),
            without_pass_rate: None,
            skill_words: None,
            walk_secs: None,
        },
        &[NewSkillEvalDimRate {
            dim: "correctness",
            graded: 2,
            passed: 1,
            without_passed: None,
        }],
        0.75,
    )
    .await;

    // Newest run: carries the baseline, the cost proxies, and two dimensions.
    record(
        &conn,
        "harness",
        NewSkillEvalRun {
            skill_name: "harness",
            mode: EvalMode::Deterministic,
            graded: 4,
            passed: 3,
            pass_rate: Some(0.75),
            without_pass_rate: Some(0.25),
            skill_words: Some(754),
            walk_secs: Some(3.25),
        },
        &[
            NewSkillEvalDimRate {
                dim: "correctness",
                graded: 2,
                passed: 1,
                without_passed: Some(0),
            },
            NewSkillEvalDimRate {
                dim: "efficiency",
                graded: 2,
                passed: 2,
                without_passed: Some(1),
            },
        ],
        0.75,
    )
    .await;

    raise_skill_bar(&conn, "harness", EvalMode::Deterministic, 0.5)
        .await
        .unwrap();
    raise_lift_floor(&conn, "harness", EvalMode::Deterministic, 0.1)
        .await
        .unwrap();

    let trends = collect_skill_trends(&conn, None, None).await.unwrap();
    assert_eq!(trends.len(), 1, "one row per skill");
    let trend = &trends[0];
    assert_eq!(trend.name, "harness");
    assert_eq!(trend.runs, 2, "both runs count");
    // `latest_pass_rate` is the skill_evals row's rate, `best_pass_rate` is the
    // best the run history ever recorded: the two sources must not be conflated.
    assert_eq!(trend.latest_pass_rate, Some(0.75));
    assert_eq!(trend.best_pass_rate, Some(0.75));
    assert_eq!(trend.bar_floor, Some(0.5));
    assert_eq!(trend.lift_floor, Some(0.1));
    // Lift is measured on the newest run only: 0.75 - 0.25.
    assert_eq!(trend.lift, Some(0.5));
    assert_eq!(trend.mode, Some(EvalMode::Deterministic));
    assert_eq!(trend.skill_words, Some(754));
    assert_eq!(trend.walk_secs, Some(3.25));

    // Dimension rates come from the newest run, ordered by dimension, and the
    // without-rate is computed against the same graded denominator.
    let dims: Vec<(&str, Option<f64>, Option<f64>)> = trend
        .dims
        .iter()
        .map(|dim| (dim.dim.as_str(), dim.rate, dim.without_rate))
        .collect();
    assert_eq!(
        dims,
        vec![
            ("correctness", Some(0.5), Some(0.0)),
            ("efficiency", Some(1.0), Some(0.5)),
        ]
    );

    // The accessors the aggregation reads must agree with what it reported.
    assert_eq!(
        get_skill_bar(&conn, "harness", EvalMode::Deterministic)
            .await
            .unwrap(),
        Some(0.5)
    );
    assert_eq!(
        get_lift_floor(&conn, "harness", EvalMode::Deterministic)
            .await
            .unwrap(),
        Some(0.1)
    );
}

/// Without a run whose baseline was measured there is no lift, and the
/// skill-scoped filter must select exactly the requested skill.
#[tokio::test(flavor = "current_thread")]
async fn trends_report_no_lift_without_a_baseline_and_honour_the_filter() {
    let (_dir, conn) = open().await;
    for skill in ["harness", "pr-triage"] {
        record(
            &conn,
            skill,
            NewSkillEvalRun {
                skill_name: skill,
                mode: EvalMode::Deterministic,
                graded: 2,
                passed: 1,
                pass_rate: Some(0.5),
                without_pass_rate: None,
                skill_words: Some(100),
                walk_secs: Some(1.0),
            },
            &[],
            0.5,
        )
        .await;
    }

    let all = collect_skill_trends(&conn, None, None).await.unwrap();
    assert_eq!(
        all.iter()
            .map(|trend| trend.name.as_str())
            .collect::<Vec<_>>(),
        vec!["harness", "pr-triage"],
        "trends are ordered by skill name"
    );
    assert_eq!(
        all[0].lift, None,
        "an unmeasured baseline must report no lift rather than zero"
    );
    assert_eq!(
        all[0].bar_floor, None,
        "no blessed floor is not a floor of 0"
    );

    let filtered = collect_skill_trends(&conn, None, Some("pr-triage"))
        .await
        .unwrap();
    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].name, "pr-triage");
}

/// A run that graded nothing reports no rates at all, and a dimension with no
/// baseline reports no without-rate: `None` must not collapse to `0.0`, which
/// would read as "everything failed" in a report.
#[tokio::test(flavor = "current_thread")]
async fn ungraded_runs_and_dimensions_report_none() {
    let (_dir, conn) = open().await;
    let run_id = insert_skill_eval_run(
        &conn,
        &NewSkillEvalRun {
            skill_name: "harness",
            mode: EvalMode::Agent,
            graded: 0,
            passed: 0,
            pass_rate: None,
            without_pass_rate: None,
            skill_words: None,
            walk_secs: None,
        },
    )
    .await
    .unwrap();
    do_harness_db::insert_dim_rates(
        &conn,
        run_id,
        &[NewSkillEvalDimRate {
            dim: "security",
            graded: 0,
            passed: 0,
            without_passed: Some(0),
        }],
    )
    .await
    .unwrap();

    let trends = collect_skill_trends(&conn, None, None).await.unwrap();
    let trend = trends
        .iter()
        .find(|trend| trend.name == "harness")
        .expect("a run with no rate still produces a trend row");
    assert_eq!(trend.latest_pass_rate, None);
    let dim = trend.dims.first().expect("dimension row");
    assert_eq!(dim.dim, "security");
    assert_eq!(
        dim.rate, None,
        "a dimension that graded nothing has no rate"
    );
    assert_eq!(
        dim.without_rate, None,
        "no denominator means no without-rate either"
    );
}

/// Every rate the report derives is finite or absent.
///
/// S17 names the bare `f64` pass-rate fields: a `NaN` would poison the ratchet's
/// ordering comparisons, since every comparison with `NaN` is false, so "best
/// ever" and "lift floor" would turn into coin flips. The derivation guards its
/// divisions (`graded > 0`), so `NaN` cannot enter through the arithmetic; this
/// pins that property from the outside, at both extremes and at the empty
/// denominator.
#[tokio::test(flavor = "current_thread")]
async fn derived_rates_are_finite_or_absent() {
    let (_dir, conn) = open().await;
    for (skill, graded, passed) in [
        ("all-pass", 3, 3),
        ("none-pass", 3, 0),
        ("nothing-graded", 0, 0),
    ] {
        record(
            &conn,
            skill,
            NewSkillEvalRun {
                skill_name: skill,
                mode: EvalMode::Deterministic,
                graded,
                passed,
                pass_rate: (graded > 0).then(|| passed as f64 / graded as f64),
                without_pass_rate: None,
                skill_words: None,
                walk_secs: None,
            },
            &[NewSkillEvalDimRate {
                dim: "correctness",
                graded,
                passed,
                without_passed: Some(passed),
            }],
            if graded > 0 {
                passed as f64 / graded as f64
            } else {
                0.0
            },
        )
        .await;
    }

    let trends = collect_skill_trends(&conn, None, None).await.unwrap();
    assert_eq!(trends.len(), 3, "one trend row per seeded skill");
    for trend in trends {
        assert!(
            trend.latest_pass_rate.is_none_or(f64::is_finite),
            "{}: latest pass rate",
            trend.name
        );
        assert!(
            trend.best_pass_rate.is_none_or(f64::is_finite),
            "{}: best pass rate",
            trend.name
        );
        assert!(
            trend.lift.is_none_or(f64::is_finite),
            "{}: lift",
            trend.name
        );
        assert!(
            trend.bar_floor.is_none_or(f64::is_finite),
            "{}: bar floor",
            trend.name
        );
        for dim in &trend.dims {
            assert!(
                dim.rate.is_none_or(f64::is_finite),
                "{}: {} rate",
                trend.name,
                dim.dim
            );
            assert!(
                dim.without_rate.is_none_or(f64::is_finite),
                "{}: {} without-rate",
                trend.name,
                dim.dim
            );
        }
    }
}
