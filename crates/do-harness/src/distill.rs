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
    println!("Distilled heuristic {id} for {skill}");
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

    #[tokio::test(flavor = "current_thread")]
    async fn distill_inserts_heuristic_from_resolved_trace() {
        let dir = tempfile::tempdir().unwrap();
        write_skill_md(dir.path(), "harness");
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
