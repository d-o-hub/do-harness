//! Regression tests for #419: `evals-invalid` must not exit 0.

use super::*;

/// The reproduction from the issue: the schema rejects `skill` (the field is
/// `skill_name`).
const UNKNOWN_FIELD_JSON: &str = r#"{
  "skill": "test-skill",
  "evals": []
}"#;

#[tokio::test(flavor = "current_thread")]
async fn invalid_evals_fail_the_run_without_persisting_a_score() {
    let dir = fixture_root(VALID_SKILL_MD, Some(UNKNOWN_FIELD_JSON));

    let err = eval_run(dir.path(), None, false).await.unwrap_err();
    assert!(err.to_string().contains("test-skill"), "{err:#}");
    assert_eq!(persisted(dir.path()).await.len(), 0);
}

#[tokio::test(flavor = "current_thread")]
async fn invalid_evals_are_flagged_on_the_report() {
    let dir = fixture_root(VALID_SKILL_MD, Some(UNKNOWN_FIELD_JSON));
    let skill_dir = dir.path().join(".agents/skills/test-skill");
    let gate = dir
        .path()
        .join(".agents/skills/skill-creator/scripts/quick_validate.py");

    match grading::gate_and_parse(&skill_dir, "test-skill", &gate)
        .await
        .unwrap()
    {
        grading::GateOutcome::InvalidEvals(report) => assert!(report.evals_invalid),
        _ => panic!("expected InvalidEvals for an unknown fixture field"),
    }
}

/// A skill that ships no fixture has nothing to grade; only an unparsable
/// fixture is an input failure.
#[tokio::test(flavor = "current_thread")]
async fn missing_evals_still_succeed() {
    let dir = fixture_root(VALID_SKILL_MD, None);

    eval_run(dir.path(), None, false).await.unwrap();
}

#[tokio::test(flavor = "current_thread")]
async fn bless_refuses_an_invalid_fixture_without_recording_a_baseline() {
    let dir = fixture_root(VALID_SKILL_MD, Some(UNKNOWN_FIELD_JSON));

    let err = eval_run(dir.path(), None, true).await.unwrap_err();
    assert!(err.to_string().contains("does not parse"), "{err:#}");
    let conn = do_harness_db::connect_and_migrate(dir.path())
        .await
        .unwrap();
    assert!(
        do_harness_db::get_grader_baseline(&conn, "test-skill")
            .await
            .unwrap()
            .is_none()
    );
}
