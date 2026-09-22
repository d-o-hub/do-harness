//! Tests for `distill.rs`, extracted to keep that file under the 450-line decomposition threshold.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;

use super::*;

fn write_skill_md(root: &Path, skill: &str) {
    let dir = root.join(".agents").join("skills").join(skill);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("SKILL.md"), format!("# {skill}\n")).unwrap();
}

async fn seed_trace(root: &Path, session: &str, resolution_steps: Option<&str>) -> i64 {
    let conn = do_harness_db::connect_and_migrate(root).await.unwrap();
    do_harness_db::insert_trace(
        &conn,
        &do_harness_db::NewTrace {
            task_id: None,
            session_id: session,
            command: Some("cargo check"),
            error_diff: Some("E0308"),
            resolution_steps,
        },
    )
    .await
    .unwrap()
}

async fn seed_ok_beat(root: &Path) {
    let conn = do_harness_db::connect_and_migrate(root).await.unwrap();
    do_harness_db::record_sensor_outcome(
        &conn,
        &do_harness_db::NewBeat {
            task_id: None,
            beat_type: "sensor",
            status: "ok",
            sensor_exit_code: Some(0),
            sensor_name: Some("check"),
            started_at: 1,
            completed_at: Some(1),
        },
        true,
        None,
    )
    .await
    .unwrap();
}

#[tokio::test(flavor = "current_thread")]
async fn distill_inserts_heuristic_from_resolved_trace() {
    let dir = tempfile::tempdir().unwrap();
    write_skill_md(dir.path(), "harness");
    seed_ok_beat(dir.path()).await;
    let trace_id = seed_trace(dir.path(), "s1", Some("applied self-correction protocol")).await;

    distill(
        dir.path(),
        "harness",
        "classify sensor failure before fixing",
        Some("fires when clippy reports a missing lifetime"),
        Some(trace_id),
        false,
        false,
        Format::Text,
    )
    .await
    .unwrap();

    let conn = do_harness_db::connect_and_migrate(dir.path())
        .await
        .unwrap();
    let heuristics = do_harness_db::list_heuristics(&conn, "harness")
        .await
        .unwrap();
    assert_eq!(heuristics.len(), 1);
    assert_eq!(
        heuristics[0].pattern,
        "classify sensor failure before fixing"
    );
    assert_eq!(heuristics[0].source_trace_id, Some(trace_id));
}

#[tokio::test(flavor = "current_thread")]
async fn distill_appends_to_heuristics_md_and_skill_pointer() {
    let dir = tempfile::tempdir().unwrap();
    write_skill_md(dir.path(), "harness");
    seed_ok_beat(dir.path()).await;
    let trace_id = seed_trace(dir.path(), "s1", Some("fixed lifetime")).await;

    distill(
        dir.path(),
        "harness",
        "add explicit lifetime bounds",
        Some("applies when the borrow checker flags a missing bound"),
        Some(trace_id),
        false,
        false,
        Format::Text,
    )
    .await
    .unwrap();

    let skill_dir = dir.path().join(".agents").join("skills").join("harness");
    let heuristics =
        fs::read_to_string(skill_dir.join("references").join("heuristics.md")).unwrap();
    assert!(heuristics.starts_with("# Heuristics\n"));
    assert!(heuristics.contains(
        "- **add explicit lifetime bounds**: applies when the borrow checker flags a missing bound (from trace "
    ));
    let skill_md = fs::read_to_string(skill_dir.join("SKILL.md")).unwrap();
    assert!(skill_md.contains("## Guides\nSee references/heuristics.md for distilled heuristics."));
}

#[tokio::test(flavor = "current_thread")]
async fn distill_skill_pointer_is_idempotent() {
    let dir = tempfile::tempdir().unwrap();
    write_skill_md(dir.path(), "harness");
    seed_ok_beat(dir.path()).await;
    let trace_id = seed_trace(dir.path(), "s1", Some("fixed lifetime")).await;

    distill(
        dir.path(),
        "harness",
        "one",
        None,
        Some(trace_id),
        false,
        false,
        Format::Text,
    )
    .await
    .unwrap();
    distill(
        dir.path(),
        "harness",
        "two",
        None,
        Some(trace_id),
        false,
        false,
        Format::Text,
    )
    .await
    .unwrap();

    let skill_md = fs::read_to_string(
        dir.path()
            .join(".agents")
            .join("skills")
            .join("harness")
            .join("SKILL.md"),
    )
    .unwrap();
    assert_eq!(
        skill_md
            .matches("See references/heuristics.md for distilled heuristics.")
            .count(),
        1
    );
}

#[tokio::test(flavor = "current_thread")]
async fn distill_refuses_without_ok_beat() {
    let dir = tempfile::tempdir().unwrap();
    write_skill_md(dir.path(), "harness");
    let trace_id = seed_trace(dir.path(), "s1", Some("fixed lifetime")).await;

    let err = distill(
        dir.path(),
        "harness",
        "p",
        None,
        Some(trace_id),
        false,
        false,
        Format::Text,
    )
    .await
    .unwrap_err();
    assert_eq!(
        err.to_string(),
        "distill requires evidence: no ok sensor beat recorded (run do-harness verify --record)"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn distill_refuses_without_from_trace() {
    let dir = tempfile::tempdir().unwrap();
    let err = distill(
        dir.path(),
        "harness",
        "p",
        None,
        None,
        false,
        false,
        Format::Text,
    )
    .await
    .unwrap_err();
    assert_eq!(
        err.to_string(),
        "distill requires evidence: pass --from-trace <id> of a resolved trace (see do-harness trace add)"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn distill_refuses_unknown_skill() {
    let dir = tempfile::tempdir().unwrap();
    let err = distill(
        dir.path(),
        "nope",
        "p",
        None,
        Some(1),
        false,
        false,
        Format::Text,
    )
    .await
    .unwrap_err();
    assert_eq!(
        err.to_string(),
        "unknown skill 'nope': no SKILL.md under .agents/skills"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn distill_refuses_missing_trace() {
    let dir = tempfile::tempdir().unwrap();
    write_skill_md(dir.path(), "harness");
    let err = distill(
        dir.path(),
        "harness",
        "p",
        None,
        Some(999),
        false,
        false,
        Format::Text,
    )
    .await
    .unwrap_err();
    assert_eq!(err.to_string(), "trace 999 not found");
}

#[tokio::test(flavor = "current_thread")]
async fn distill_refuses_trace_without_resolution_steps() {
    let dir = tempfile::tempdir().unwrap();
    write_skill_md(dir.path(), "harness");
    let trace_id = seed_trace(dir.path(), "s1", None).await;

    let err = distill(
        dir.path(),
        "harness",
        "p",
        None,
        Some(trace_id),
        false,
        false,
        Format::Text,
    )
    .await
    .unwrap_err();
    assert_eq!(
        err.to_string(),
        format!(
            "trace {trace_id} has no resolution steps; record the verified fix with do-harness trace add --resolution-steps before distilling"
        )
    );
}

#[tokio::test(flavor = "current_thread")]
async fn distill_refuses_trace_with_empty_resolution_steps() {
    let dir = tempfile::tempdir().unwrap();
    write_skill_md(dir.path(), "harness");
    let trace_id = seed_trace(dir.path(), "s1", Some("")).await;

    let err = distill(
        dir.path(),
        "harness",
        "p",
        None,
        Some(trace_id),
        false,
        false,
        Format::Text,
    )
    .await
    .unwrap_err();
    assert_eq!(
        err.to_string(),
        format!(
            "trace {trace_id} has no resolution steps; record the verified fix with do-harness trace add --resolution-steps before distilling"
        )
    );
}

#[tokio::test(flavor = "current_thread")]
async fn distill_accepts_resolution_steps_with_anti_ai_slop_checklist() {
    let dir = tempfile::tempdir().unwrap();
    write_skill_md(dir.path(), "harness");
    seed_ok_beat(dir.path()).await;
    let trace_id = seed_trace(
        dir.path(),
        "s1",
        Some("anti-ai-slop: pass (no speculative structs, real error handling)"),
    )
    .await;

    distill(
        dir.path(),
        "harness",
        "idiomatic error context wrapping",
        Some("applies when adding context to I/O failures"),
        Some(trace_id),
        false,
        false,
        Format::Text,
    )
    .await
    .unwrap();

    let conn = do_harness_db::connect_and_migrate(dir.path())
        .await
        .unwrap();
    let trace = do_harness_db::get_trace(&conn, trace_id)
        .await
        .unwrap()
        .unwrap();
    assert!(
        trace
            .resolution_steps
            .as_deref()
            .unwrap()
            .contains("anti-ai-slop: pass")
    );

    let heuristics = do_harness_db::list_heuristics(&conn, "harness")
        .await
        .unwrap();
    assert_eq!(heuristics.len(), 1);
    assert_eq!(heuristics[0].pattern, "idiomatic error context wrapping");
}
