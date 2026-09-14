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

    // Invalid fixture surfaces evals-invalid without persisting a score.
    eval_run_with_lift(dir.path(), None).await.unwrap();
    assert!(persisted(dir.path()).await.is_empty());
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
