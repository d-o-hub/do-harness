//! Heuristic distillation from resolved traces, for `do-harness distill`.

use std::path::Path;

use anyhow::Result;

/// Distills a heuristic from a resolved trace into a skill.
///
/// Evidence guardrail: `from_trace` must point at a trace whose resolution
/// steps were recorded via `do-harness trace add --resolution-steps`, and the
/// skill must already exist under `root/.agents/skills/<skill>/SKILL.md`.
/// Inserts the heuristic row and prints its id.
///
/// # Errors
///
/// Returns an error when evidence is missing, the skill is unknown, the trace
/// cannot be loaded, the trace lacks resolution steps, or the insert fails.
pub async fn distill(
    root: &Path,
    skill: &str,
    pattern: &str,
    description: Option<&str>,
    from_trace: Option<i64>,
) -> Result<()> {
    let Some(trace_id) = from_trace else {
        anyhow::bail!(
            "distill requires evidence: pass --from-trace <id> of a resolved trace (see do-harness trace add)"
        );
    };
    let skill_md = root
        .join(".agents")
        .join("skills")
        .join(skill)
        .join("SKILL.md");
    if !skill_md.exists() {
        anyhow::bail!("unknown skill '{skill}': no SKILL.md under .agents/skills");
    }
    let conn = do_harness_db::connect_and_migrate(root).await?;
    let trace = do_harness_db::get_trace(&conn, trace_id).await?;
    let Some(trace) = trace else {
        anyhow::bail!("trace {trace_id} not found");
    };
    if trace.resolution_steps.as_deref().is_none_or(str::is_empty) {
        anyhow::bail!(
            "trace {trace_id} has no resolution steps; record the verified fix with do-harness trace add --resolution-steps before distilling"
        );
    }
    let beats = do_harness_db::list_beats(&conn, None).await?;
    if !beats.iter().any(|beat| beat.status == "ok") {
        anyhow::bail!(
            "distill requires evidence: no ok sensor beat recorded (run do-harness verify --record)"
        );
    }
    let id = do_harness_db::insert_heuristic(
        &conn,
        &do_harness_db::NewHeuristic {
            skill_name: skill,
            pattern,
            description,
            source_trace_id: Some(trace_id),
        },
    )
    .await?;
    crate::skill_write::append_heuristic(root, skill, pattern, description, trace_id)?;
    crate::skill_write::ensure_skill_pointer(root, skill)?;
    println!("Distilled heuristic {id} for {skill}; appended to references/heuristics.md");
    Ok(())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use std::fs;

    use super::*;

    fn write_skill_md(root: &Path, skill: &str) {
        let dir = root.join(".agents").join("skills").join(skill);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("SKILL.md"), "# {skill}\n").unwrap();
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
        do_harness_db::insert_beat(
            &conn,
            &do_harness_db::NewBeat {
                task_id: None,
                beat_type: "sensor",
                status: "ok",
                sensor_exit_code: Some(0),
                started_at: 1,
                completed_at: Some(1),
            },
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
        assert!(
            skill_md.contains("## Guides\nSee references/heuristics.md for distilled heuristics.")
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn distill_skill_pointer_is_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        write_skill_md(dir.path(), "harness");
        seed_ok_beat(dir.path()).await;
        let trace_id = seed_trace(dir.path(), "s1", Some("fixed lifetime")).await;

        distill(dir.path(), "harness", "one", None, Some(trace_id))
            .await
            .unwrap();
        distill(dir.path(), "harness", "two", None, Some(trace_id))
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

        let err = distill(dir.path(), "harness", "p", None, Some(trace_id))
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
        let err = distill(dir.path(), "harness", "p", None, None)
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
        let err = distill(dir.path(), "nope", "p", None, Some(1))
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
        let err = distill(dir.path(), "harness", "p", None, Some(999))
            .await
            .unwrap_err();
        assert_eq!(err.to_string(), "trace 999 not found");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn distill_refuses_trace_without_resolution_steps() {
        let dir = tempfile::tempdir().unwrap();
        write_skill_md(dir.path(), "harness");
        let trace_id = seed_trace(dir.path(), "s1", None).await;

        let err = distill(dir.path(), "harness", "p", None, Some(trace_id))
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

        let err = distill(dir.path(), "harness", "p", None, Some(trace_id))
            .await
            .unwrap_err();
        assert_eq!(
            err.to_string(),
            format!(
                "trace {trace_id} has no resolution steps; record the verified fix with do-harness trace add --resolution-steps before distilling"
            )
        );
    }
}
