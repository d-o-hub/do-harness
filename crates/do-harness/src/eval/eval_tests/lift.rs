//! Lift, dimension, and floor tests for the eval runner.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;

async fn eval_run_with_lift(dir: &Path, skill: Option<&str>) -> Result<()> {
    run_eval(
        dir,
        EvalOpts {
            skill,
            bless: false,
            list_skills: false,
            fail_fast: false,
            dry_run: false,
            format: Format::Text,
            approver: Some("test-approver"),
            no_lift: false,
            agent_cmd: None,
            agent_timeout_secs: 600,
            strict_fixtures: false,
            trials: 1,
        },
    )
    .await
}

async fn eval_run_with_bless(dir: &Path, skill: Option<&str>) -> Result<()> {
    run_eval(
        dir,
        EvalOpts {
            skill,
            bless: true,
            list_skills: false,
            fail_fast: false,
            dry_run: false,
            format: Format::Text,
            approver: Some("test-approver"),
            no_lift: false,
            agent_cmd: None,
            agent_timeout_secs: 600,
            strict_fixtures: false,
            trials: 1,
        },
    )
    .await
}

#[tokio::test(flavor = "current_thread")]
async fn lift_measures_guidance_dependence_and_persists_dims() {
    let dir = fixture_root(
        VALID_SKILL_MD,
        Some(
            r#"{
          "skill_name": "test-skill",
          "evals": [
            {
              "id": 1,
              "prompt": "guided task",
              "expected_output": "guided out",
              "files": [],
              "dim": "discoverability",
              "assertions": [
                "contains:.agents/skills/test-skill/SKILL.md|fixture skill",
                "exists:."
              ]
            }
          ]
        }"#,
        ),
    );

    eval_run_with_lift(dir.path(), None).await.unwrap();
    let conn = do_harness_db::connect_and_migrate(dir.path())
        .await
        .unwrap();
    let run = do_harness_db::latest_eval_run(&conn, "test-skill")
        .await
        .unwrap()
        .unwrap();
    // With skill 2/2; without skill the SKILL.md assertion fails: 1/2.
    assert_eq!(run.mode, do_harness_types::EvalMode::Deterministic);
    assert_eq!(run.pass_rate, Some(1.0));
    assert_eq!(run.without_pass_rate, Some(0.5));
    assert!(run.skill_words.unwrap_or(0) > 0);
    assert!(run.walk_secs.unwrap_or(-1.0) >= 0.0);
    let rates = do_harness_db::dim_rates_for_run(&conn, run.id)
        .await
        .unwrap();
    assert_eq!(rates.len(), 1);
    assert_eq!(rates[0].dim, "discoverability");
    assert_eq!(rates[0].graded, 2);
    assert_eq!(rates[0].passed, 2);
    assert_eq!(rates[0].without_passed, Some(1));
}

#[tokio::test(flavor = "current_thread")]
async fn no_lift_flag_skips_baseline() {
    let dir = fixture_root(VALID_SKILL_MD, Some(&single_case_json(&["exists:."])));

    eval_run(dir.path(), None, false).await.unwrap();
    let conn = do_harness_db::connect_and_migrate(dir.path())
        .await
        .unwrap();
    let run = do_harness_db::latest_eval_run(&conn, "test-skill")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(run.pass_rate, Some(1.0));
    assert_eq!(run.without_pass_rate, None);
    for rate in do_harness_db::dim_rates_for_run(&conn, run.id)
        .await
        .unwrap()
    {
        assert_eq!(rate.without_passed, None);
    }
}

#[tokio::test(flavor = "current_thread")]
async fn unknown_dim_fails_fixture_closed() {
    let dir = fixture_root(
        VALID_SKILL_MD,
        Some(
            r#"{
          "skill_name": "test-skill",
          "evals": [
            {
              "id": 1,
              "prompt": "p",
              "expected_output": "o",
              "files": [],
              "dim": "bogus",
              "assertions": ["exists:."]
            }
          ]
        }"#,
        ),
    );

    // An unparsable fixture fails the run and persists no score.
    let err = eval_run_with_lift(dir.path(), None).await.unwrap_err();
    assert!(err.to_string().contains("test-skill"), "{err:#}");
    assert_eq!(persisted(dir.path()).await.len(), 0);
}

#[tokio::test(flavor = "current_thread")]
async fn lift_miss_fails_below_floor() {
    let dir = fixture_root(VALID_SKILL_MD, Some(&single_case_json(&["exists:."])));
    let conn = do_harness_db::connect_and_migrate(dir.path())
        .await
        .unwrap();
    do_harness_db::raise_lift_floor(
        &conn,
        "test-skill",
        do_harness_types::EvalMode::Deterministic,
        0.5,
    )
    .await
    .unwrap();
    drop(conn);

    // With and without both score 1.0, so lift 0.0 misses the 0.5 floor.
    let err = eval_run_with_lift(dir.path(), None).await.unwrap_err();
    assert!(err.to_string().contains("test-skill"), "{err:#}");
}

#[tokio::test(flavor = "current_thread")]
async fn lift_at_floor_passes() {
    let dir = fixture_root(VALID_SKILL_MD, Some(&single_case_json(&["exists:."])));
    let conn = do_harness_db::connect_and_migrate(dir.path())
        .await
        .unwrap();
    do_harness_db::raise_lift_floor(
        &conn,
        "test-skill",
        do_harness_types::EvalMode::Deterministic,
        -1.0,
    )
    .await
    .unwrap();
    drop(conn);

    eval_run_with_lift(dir.path(), None).await.unwrap();
}

/// The lift floor belongs to a fixture set, not to a skill forever. A re-bless
/// after the graders change must start the new set's floor from its own blessed
/// lift instead of ratcheting against a floor measured on assertions that no
/// longer exist — otherwise a legitimate fixture change makes the floor
/// permanently unreachable and `eval --strict-fixtures` can never pass again.
#[tokio::test(flavor = "current_thread")]
async fn rebless_after_a_fixture_change_resets_the_lift_floor() {
    let dir = fixture_root(
        VALID_SKILL_MD,
        Some(&single_case_json(&[
            "contains:.agents/skills/test-skill/SKILL.md|fixture skill",
            "exists:.",
        ])),
    );
    // First fixture set: with skill 2/2, without skill 1/2, so lift is 0.5.
    eval_run_with_bless(dir.path(), None).await.unwrap();
    let conn = do_harness_db::connect_and_migrate(dir.path())
        .await
        .unwrap();
    let floor = do_harness_db::get_lift_floor(
        &conn,
        "test-skill",
        do_harness_types::EvalMode::Deterministic,
    )
    .await
    .unwrap()
    .expect("blessing must set a lift floor");
    assert!(
        (floor - 0.45).abs() < 1e-9,
        "unexpected first floor {floor}"
    );
    drop(conn);

    // The graders change: the skill-dependent assertion is gone, so both runs
    // score 1.0 and lift is 0.0. That is a legitimate drop, not erosion.
    fs::write(
        dir.path()
            .join(".agents/skills/test-skill/evals/evals.json"),
        single_case_json(&["exists:."]),
    )
    .unwrap();
    eval_run_with_bless(dir.path(), None).await.unwrap();

    let conn = do_harness_db::connect_and_migrate(dir.path())
        .await
        .unwrap();
    let floor = do_harness_db::get_lift_floor(
        &conn,
        "test-skill",
        do_harness_types::EvalMode::Deterministic,
    )
    .await
    .unwrap()
    .expect("the new fixture set must have a floor");
    assert!(
        floor < 0.0,
        "the new fixture set's floor must come from its own lift, got {floor}"
    );
    drop(conn);

    // And the eval passes instead of failing LIFT-MISS forever.
    eval_run_with_lift(dir.path(), None).await.unwrap();
}

/// The fixture-set boundary is the only place the floor may fall. Re-blessing
/// the *same* graders must keep ratcheting, so guidance erosion is still caught
/// and the reset above cannot be used as a general escape hatch.
#[tokio::test(flavor = "current_thread")]
async fn rebless_with_unchanged_graders_keeps_the_ratchet() {
    let dir = fixture_root(
        VALID_SKILL_MD,
        Some(&single_case_json(&[
            "contains:.agents/skills/test-skill/SKILL.md|fixture skill",
            "exists:.",
        ])),
    );
    eval_run_with_bless(dir.path(), None).await.unwrap();

    // Raise the floor above what this fixture set's lift would derive.
    let conn = do_harness_db::connect_and_migrate(dir.path())
        .await
        .unwrap();
    do_harness_db::raise_lift_floor(
        &conn,
        "test-skill",
        do_harness_types::EvalMode::Deterministic,
        0.9,
    )
    .await
    .unwrap();
    drop(conn);

    // Same graders, so the re-bless must not lower the floor back to 0.45.
    eval_run_with_bless(dir.path(), None).await.unwrap();
    let conn = do_harness_db::connect_and_migrate(dir.path())
        .await
        .unwrap();
    let floor = do_harness_db::get_lift_floor(
        &conn,
        "test-skill",
        do_harness_types::EvalMode::Deterministic,
    )
    .await
    .unwrap()
    .expect("floor");
    assert!(
        (floor - 0.9).abs() < 1e-9,
        "an unchanged fixture set must keep its ratchet, got {floor}"
    );
}
