//! Heuristic distillation from resolved traces, for `do-harness distill`.

use std::path::Path;

use anyhow::Result;

use crate::report::Format;

/// Distills a heuristic from a resolved trace into a skill.
#[allow(clippy::too_many_arguments)]
pub async fn distill(
    root: &Path,
    skill: &str,
    pattern: &str,
    description: Option<&str>,
    from_trace: Option<i64>,
    to_fixture: bool,
    dry_run: bool,
    format: Format,
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
    let beats_ok = do_harness_db::has_ok_beat(&conn).await?;
    if !beats_ok {
        anyhow::bail!(
            "distill requires evidence: no ok sensor beat recorded (run do-harness verify --record)"
        );
    }

    if dry_run {
        println!("Dry run: would distill heuristic for {skill} from trace {trace_id}: '{pattern}'");
        return Ok(());
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

    if format == Format::Json {
        let json = serde_json::json!({
            "id": id,
            "skill": skill,
            "pattern": pattern,
            "description": description,
            "from_trace": trace_id,
            "to_fixture": to_fixture
        });
        println!("{json}");
    } else {
        println!("Distilled heuristic {id} for {skill}; appended to references/heuristics.md");
    }

    if to_fixture {
        raise_bar_from_recovery(&conn, skill).await?;
    }

    // Durable link: the event names the tracked reference the heuristic landed
    // in, so the trace -> guide path survives even though the event file is
    // gitignored local state.
    let guides = [
        crate::events::GuideRef::reference(skill, "heuristics.md"),
        crate::events::GuideRef::skill(skill),
    ];
    let payload = serde_json::json!({
        "trigger": "resolved-trace",
        "heuristic_id": id,
        "pattern": pattern,
        "from_trace": trace_id,
        "to_fixture": to_fixture,
    });
    crate::events::write_event(root, "distill", payload, &guides).await?;
    Ok(())
}

async fn raise_bar_from_recovery(conn: &do_harness_db::Connection, skill: &str) -> Result<()> {
    let best = do_harness_db::max_pass_rate(conn, skill).await?;
    match do_harness_integrity_floor(best) {
        Some(floor) => {
            // Recovery ratchets the deterministic floor: distill evidence
            // comes from sensor runs, not agent sessions.
            if do_harness_db::raise_skill_bar(
                conn,
                skill,
                do_harness_types::EvalMode::Deterministic,
                floor,
            )
            .await?
            {
                println!("Bar ratcheted for {skill}: floor now {floor:.2}");
            } else {
                println!("Bar unchanged for {skill}: floor already at or above {floor:.2}");
            }
        }
        None => {
            println!(
                "No eval history for {skill}; nothing to ratchet yet (run do-harness eval first)"
            );
        }
    }
    Ok(())
}

fn do_harness_integrity_floor(best_ever: Option<f64>) -> Option<f64> {
    crate::eval_integrity::GraderHashes::bar_floor(best_ever)
}

#[cfg(test)]
#[path = "distill_tests.rs"]
mod tests;
