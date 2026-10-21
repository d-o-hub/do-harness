use super::*;

#[tokio::test(flavor = "current_thread")]
async fn unknown_fixture_field_reports_closed_schema_error() {
    let json = r#"{
      "skill_name": "test-skill",
      "evals": [
        {
          "id": 1,
          "prompt": "prompt one",
          "expected_output": "out one",
          "files": [],
          "assertions": ["exists:."],
          "future_field": true
        }
      ]
    }"#;
    let dir = fixture_root(VALID_SKILL_MD, Some(json));
    let skill_dir = dir.path().join(".agents/skills/test-skill");
    let gate = dir
        .path()
        .join(".agents/skills/skill-creator/scripts/quick_validate.py");
    match grading::gate_and_parse(&skill_dir, "test-skill", &gate)
        .await
        .unwrap()
    {
        grading::GateOutcome::InvalidEvals(report) => {
            assert!(report.line.contains("unknown field"), "{}", report.line);
            assert!(report.line.contains("unsupported field"), "{}", report.line);
            assert!(
                !report.line.contains("upgrade and re-run"),
                "{}",
                report.line
            );
        }
        _ => panic!("expected InvalidEvals for an unknown fixture field"),
    }
}

#[tokio::test(flavor = "current_thread")]
async fn evals_with_version_reach_ready_gate() {
    let json = r#"{
      "skill_name": "test-skill",
      "version": "1.1.0",
      "evals": []
    }"#;
    let dir = fixture_root(VALID_SKILL_MD, Some(json));
    let skill_dir = dir.path().join(".agents/skills/test-skill");
    let gate = dir
        .path()
        .join(".agents/skills/skill-creator/scripts/quick_validate.py");
    match grading::gate_and_parse(&skill_dir, "test-skill", &gate)
        .await
        .unwrap()
    {
        grading::GateOutcome::Ready { evals, .. } => {
            assert_eq!(evals.version, Some(serde_json::json!("1.1.0")));
        }
        _ => panic!("expected evals with version to pass the gate and parse"),
    }
}
