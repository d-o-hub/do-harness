//! Fixture taxonomy and `--strict-fixtures` gate tests.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::eval_tests::{VALID_SKILL_MD, eval_run_strict, fixture_root, single_case_json};
use super::fixture::{AgentMode, fixture_diagnostics as fixture_diagnostics_mode};
use super::grading::SkillEvals;

fn rich_case_json() -> String {
    r#"{
      "skill_name": "test-skill",
      "evals": [
        {
          "id": 1,
          "prompt": "in scope",
          "expected_output": "handled",
          "files": [],
          "dim": "effectiveness",
          "kind": "explicit",
          "assertions": [
            "contains:.agents/skills/test-skill/SKILL.md|fixture skill",
            "cli:version:contains:do-harness"
          ]
        },
        {
          "id": 2,
          "prompt": "out of scope",
          "expected_output": "no action",
          "files": [],
          "dim": "discoverability",
          "kind": "negative",
          "assertions": ["absent:.agents/skills/test-skill/never-created"]
        }
      ]
    }"#
    .to_owned()
}

/// A fixture that grades only residue its own executor wrote: structurally
/// unable to measure Skill Lift.
fn self_answering_case_json() -> String {
    r#"{
      "skill_name": "test-skill",
      "evals": [
        {
          "id": 1,
          "prompt": "in scope",
          "expected_output": "handled",
          "files": [],
          "dim": "effectiveness",
          "kind": "explicit",
          "assertions": ["contains:residue.txt|written by the walkthrough"]
        },
        {
          "id": 2,
          "prompt": "out of scope",
          "expected_output": "no action",
          "files": [],
          "dim": "discoverability",
          "kind": "negative",
          "assertions": ["absent:.agents/skills/test-skill/never-created"]
        }
      ]
    }"#
    .to_owned()
}

#[test]
fn fixture_diagnostics_flags_thin_datasets() {
    let thin: SkillEvals = serde_json::from_str(&single_case_json(&["exists:."])).unwrap();
    let warnings = fixture_diagnostics_mode(&thin, AgentMode::Deterministic);
    assert!(
        warnings.iter().any(|w| w.contains("no negative")),
        "{warnings:?}"
    );

    let ungraded: SkillEvals = serde_json::from_str(&single_case_json(&["a human note"])).unwrap();
    let warnings = fixture_diagnostics_mode(&ungraded, AgentMode::Deterministic);
    assert!(
        warnings.iter().any(|w| w.contains("no graded assertions")),
        "{warnings:?}"
    );

    let empty: SkillEvals = serde_json::from_str(r#"{"skill_name": "s", "evals": []}"#).unwrap();
    assert_eq!(
        fixture_diagnostics_mode(&empty, AgentMode::Deterministic),
        vec!["no eval cases".to_owned()]
    );

    let rich: SkillEvals = serde_json::from_str(&rich_case_json()).unwrap();
    assert!(fixture_diagnostics_mode(&rich, AgentMode::Deterministic).is_empty());
}

/// A fixture whose assertions never read the skill's own guidance cannot
/// measure Skill Lift: the deterministic executor scores it identically with
/// and without the skill, because the walkthrough produces the residue either
/// way. The diagnostic must name that, and must clear once an assertion reads
/// SKILL.md or references/.
#[test]
fn fixture_diagnostics_flags_self_answering_fixtures() {
    let residue_only: SkillEvals = serde_json::from_str(&self_answering_case_json()).unwrap();
    let warnings = fixture_diagnostics_mode(&residue_only, AgentMode::Deterministic);
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert!(warnings[0].contains("self-answering"), "{warnings:?}");
    // Losing --strict-fixtures enforcement here is the point: the detector must
    // catch the fixture that grades its own executor's residue.
    let rich: SkillEvals = serde_json::from_str(&rich_case_json()).unwrap();
    assert!(
        fixture_diagnostics_mode(&rich, AgentMode::Deterministic).is_empty(),
        "{:?}",
        fixture_diagnostics_mode(&rich, AgentMode::Deterministic)
    );

    let guidance_reading: SkillEvals = serde_json::from_str(
        r#"{
          "skill_name": "test-skill",
          "evals": [
            {
              "id": 1,
              "prompt": "in scope",
              "expected_output": "handled",
              "files": [],
              "dim": "effectiveness",
              "kind": "explicit",
              "assertions": [
                "not-contains:.agents/skills/test-skill/references/notes.md|forbidden"
              ]
            },
            {
              "id": 2,
              "prompt": "out of scope",
              "expected_output": "no action",
              "files": [],
              "dim": "discoverability",
              "kind": "negative",
              "assertions": ["absent:.agents/skills/test-skill/never-created"]
            }
          ]
        }"#,
    )
    .unwrap();
    assert!(
        fixture_diagnostics_mode(&guidance_reading, AgentMode::Deterministic).is_empty(),
        "{:?}",
        fixture_diagnostics_mode(&guidance_reading, AgentMode::Deterministic)
    );
}

#[tokio::test(flavor = "current_thread")]
async fn strict_fixtures_rejects_self_answering_fixture() {
    let dir = fixture_root(VALID_SKILL_MD, Some(&self_answering_case_json()));
    let err = eval_run_strict(dir.path()).await.unwrap_err();
    assert!(err.to_string().contains("test-skill"), "{err:#}");
}

#[tokio::test(flavor = "current_thread")]
async fn strict_fixtures_rejects_thin_fixture() {
    let dir = fixture_root(VALID_SKILL_MD, Some(&single_case_json(&["exists:."])));
    let err = eval_run_strict(dir.path()).await.unwrap_err();
    assert!(err.to_string().contains("test-skill"), "{err:#}");
}

#[tokio::test(flavor = "current_thread")]
async fn strict_fixtures_accepts_rich_fixture() {
    let dir = fixture_root(VALID_SKILL_MD, Some(&rich_case_json()));
    eval_run_strict(dir.path()).await.unwrap();
}
