//! Tests for `distill_strikes.rs`, extracted to keep that file under the 450-line decomposition threshold.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;

fn signature(key: &str, count: i64) -> ErrorSignature {
    ErrorSignature {
        id: 1,
        signature: key.to_owned(),
        task_id: None,
        attempt_count: count,
        message: Some("boom".to_owned()),
        created_at: 1_789_430_400,
    }
}

#[test]
fn striking_filters_and_sorts_by_strikes() {
    let rows = vec![
        signature("sensor:clippy", 1),
        signature("sensor:test", 3),
        signature("sensor:fmt", 5),
    ];
    let striking = striking_signatures(&rows, 3);
    assert_eq!(striking.len(), 2);
    assert_eq!(striking[0].signature, "sensor:fmt");
    assert_eq!(striking[1].signature, "sensor:test");
}

#[test]
fn slug_strips_sensor_namespace_and_unsafe_chars() {
    assert_eq!(slug_from_signature("sensor:clippy"), "clippy");
    assert_eq!(
        slug_from_signature("sensor:eval_walk::tests::x"),
        "eval-walk-tests-x"
    );
    assert!(!slug_from_signature("!!!").is_empty());
    assert_eq!(slug_from_signature("sensor:!!!"), "sensor-strike");
}

#[test]
fn rendered_skill_md_carries_the_recorded_evidence() {
    let sig = signature("sensor:test", 4);
    let md = render_skill_md(&sig, "test");
    assert!(md.contains("name: test"));
    assert!(md.contains("`sensor:test`"));
    assert!(md.contains("Consecutive strikes: 4"));
    assert!(md.contains("boom"));
}

#[test]
fn rendered_fixture_is_gate_clean_and_demands_the_unresolved_log() {
    let sig = signature("sensor:test", 4);
    let fixture = render_evals_json(&sig, "test");
    let parsed: serde_json::Value = serde_json::from_str(&fixture).unwrap();
    let cases = parsed["evals"].as_array().unwrap();
    assert_eq!(cases.len(), 2);
    assert_eq!(cases[0]["kind"], "gotchas");
    assert_eq!(cases[1]["kind"], "negative");

    let gotchas: Vec<&str> = cases[0]["assertions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    // The starter fails because the recovery log does not exist yet; that
    // `exists:` is what makes the scaffold an honest red fixture.
    assert!(gotchas.iter().any(|a| a.starts_with("exists:")));
    assert!(gotchas.iter().any(|a| a.starts_with("not-contains:")));

    // Taxonomy check: every case must be graded, and the suite must carry
    // the out-of-scope case --strict-fixtures requires. Both come from
    // crate::eval::fixture::fixture_diagnostics, so this asserts with the
    // real diagnostic instead of a hand-copied rule.
    let all_graded = cases.iter().all(|case| {
        case["assertions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|a| crate::eval_assert::is_graded(a.as_str().unwrap()))
    });
    assert!(all_graded);
    assert!(cases.iter().any(|case| case["kind"] == "negative"));
    assert!(!fixture.contains("PLACEHOLDER"));
}

#[tokio::test(flavor = "current_thread")]
async fn scaffold_skips_an_existing_skill_and_dry_run_writes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let conn = do_harness_db::connect_and_migrate(dir.path())
        .await
        .unwrap();
    for _ in 0..3 {
        do_harness_db::bump_error_signature(&conn, "sensor:clippy", None, Some("boom"))
            .await
            .unwrap();
    }
    // A curated skill with that slug must not be overwritten.
    let existing = dir.path().join(".agents/skills/clippy");
    std::fs::create_dir_all(&existing).unwrap();
    std::fs::write(existing.join("SKILL.md"), "---\nname: clippy\n---\n").unwrap();

    let outcomes = scaffold_from_strikes(
        dir.path(),
        None,
        None,
        3,
        false,
        crate::report::Format::Json,
    )
    .await
    .unwrap();
    assert_eq!(outcomes.len(), 1);
    assert_eq!(outcomes[0].action, ScaffoldAction::ExistingSkill);
    assert_eq!(
        std::fs::read_to_string(existing.join("SKILL.md")).unwrap(),
        "---\nname: clippy\n---\n"
    );

    // A fresh signature scaffolds, and dry-run leaves no files.
    do_harness_db::clear_error_signatures(&conn, None, None)
        .await
        .unwrap();
    for _ in 0..3 {
        do_harness_db::bump_error_signature(&conn, "sensor:shell", None, Some("boom"))
            .await
            .unwrap();
    }
    let dry = scaffold_from_strikes(dir.path(), None, None, 3, true, crate::report::Format::Json)
        .await
        .unwrap();
    assert_eq!(dry.len(), 1);
    assert_eq!(dry[0].action, ScaffoldAction::WouldWrite);
    assert!(!dir.path().join(".agents/skills/shell/SKILL.md").exists());

    let real = scaffold_from_strikes(
        dir.path(),
        None,
        None,
        3,
        false,
        crate::report::Format::Json,
    )
    .await
    .unwrap();
    assert_eq!(real[0].action, ScaffoldAction::Wrote);
    assert!(dir.path().join(".agents/skills/shell/SKILL.md").exists());
    assert!(
        dir.path()
            .join(".agents/skills/shell/evals/evals.json")
            .exists()
    );
}
