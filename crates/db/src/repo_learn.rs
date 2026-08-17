//! Repository layer for the learning tables: traces, heuristics, and skill
//! evaluations.

use crate::error::Result;
use crate::migrate::unix_now;
use do_harness_types::{Heuristic, SkillEval, Trace};
use libsql::{Connection, params};

/// Insert parameters for a new trace.
#[derive(Debug, Clone)]
pub struct NewTrace<'a> {
    /// Owning task id, when the trace belongs to a task.
    pub task_id: Option<i64>,
    /// Session identifier grouping related traces.
    pub session_id: &'a str,
    /// The command that was executed.
    pub command: Option<&'a str>,
    /// Error diff or failure output captured.
    pub error_diff: Option<&'a str>,
    /// Steps taken to resolve the failure.
    pub resolution_steps: Option<&'a str>,
}

/// Insert parameters for a new heuristic.
#[derive(Debug, Clone)]
pub struct NewHeuristic<'a> {
    /// Skill the heuristic belongs to.
    pub skill_name: &'a str,
    /// Generalized pattern, stripped of project-specific identifiers.
    pub pattern: &'a str,
    /// Optional description of when the pattern applies.
    pub description: Option<&'a str>,
    /// Source trace the heuristic was distilled from.
    pub source_trace_id: Option<i64>,
}

/// Insert parameters for a new skill evaluation row.
#[derive(Debug, Clone)]
pub struct NewSkillEval<'a> {
    /// Skill the evaluation belongs to.
    pub skill_name: &'a str,
    /// The evaluation prompt.
    pub prompt: Option<&'a str>,
    /// The expected outcome of the prompt.
    pub expected_outcome: Option<&'a str>,
    /// Pass rate (fraction of graded assertions that passed), 0.0 to 1.0.
    pub pass_rate: Option<f64>,
}

/// Inserts a trace and returns its id.
///
/// # Errors
///
/// Returns an error when the insert statement fails.
pub async fn insert_trace(conn: &Connection, trace: &NewTrace<'_>) -> Result<i64> {
    conn.execute(
        "INSERT INTO traces (task_id, session_id, command, error_diff, resolution_steps, created_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params!(
            trace.task_id,
            trace.session_id,
            trace.command,
            trace.error_diff,
            trace.resolution_steps,
            unix_now()
        ),
    )
    .await?;
    Ok(conn.last_insert_rowid())
}

/// Lists traces for a session in insertion order.
///
/// # Errors
///
/// Returns an error when the query fails.
pub async fn list_traces(conn: &Connection, session_id: &str) -> Result<Vec<Trace>> {
    let mut rows = conn
        .query(
            "SELECT id, task_id, session_id, command, error_diff, resolution_steps, created_at \
             FROM traces WHERE session_id = ?1 ORDER BY id",
            params!(session_id),
        )
        .await?;
    let mut traces = Vec::new();
    while let Some(row) = rows.next().await? {
        traces.push(Trace {
            id: row.get(0)?,
            task_id: row.get(1)?,
            session_id: row.get(2)?,
            command: row.get(3)?,
            error_diff: row.get(4)?,
            resolution_steps: row.get(5)?,
            created_at: row.get(6)?,
        });
    }
    Ok(traces)
}

/// Fetches a trace by id.
///
/// # Errors
///
/// Returns an error when the query fails.
pub async fn get_trace(conn: &Connection, id: i64) -> Result<Option<Trace>> {
    let mut rows = conn
        .query(
            "SELECT id, task_id, session_id, command, error_diff, resolution_steps, created_at \
             FROM traces WHERE id = ?1",
            params!(id),
        )
        .await?;
    let row = rows.next().await?;
    let Some(row) = row else {
        return Ok(None);
    };
    Ok(Some(Trace {
        id: row.get(0)?,
        task_id: row.get(1)?,
        session_id: row.get(2)?,
        command: row.get(3)?,
        error_diff: row.get(4)?,
        resolution_steps: row.get(5)?,
        created_at: row.get(6)?,
    }))
}

/// Inserts a heuristic and returns its id.
///
/// # Errors
///
/// Returns an error when the insert statement fails.
pub async fn insert_heuristic(conn: &Connection, heuristic: &NewHeuristic<'_>) -> Result<i64> {
    conn.execute(
        "INSERT INTO heuristics (skill_name, pattern, description, source_trace_id, created_at) \
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params!(
            heuristic.skill_name,
            heuristic.pattern,
            heuristic.description,
            heuristic.source_trace_id,
            unix_now()
        ),
    )
    .await?;
    Ok(conn.last_insert_rowid())
}

/// Lists heuristics for a skill in insertion order.
///
/// # Errors
///
/// Returns an error when the query fails.
pub async fn list_heuristics(conn: &Connection, skill_name: &str) -> Result<Vec<Heuristic>> {
    let mut rows = conn
        .query(
            "SELECT id, skill_name, pattern, description, source_trace_id, created_at \
             FROM heuristics WHERE skill_name = ?1 ORDER BY id",
            params!(skill_name),
        )
        .await?;
    let mut heuristics = Vec::new();
    while let Some(row) = rows.next().await? {
        heuristics.push(Heuristic {
            id: row.get(0)?,
            skill_name: row.get(1)?,
            pattern: row.get(2)?,
            description: row.get(3)?,
            source_trace_id: row.get(4)?,
            created_at: row.get(5)?,
        });
    }
    Ok(heuristics)
}

/// Inserts or upserts a skill evaluation row and returns its id.
///
/// `skill_name` is unique; a later evaluation for the same skill overwrites the
/// previous one (latest wins), so the table holds one row per skill.
///
/// # Errors
///
/// Returns an error when the insert statement fails.
pub async fn insert_skill_eval(conn: &Connection, eval: &NewSkillEval<'_>) -> Result<i64> {
    conn.execute(
        "INSERT INTO skill_evals (skill_name, prompt, expected_outcome, pass_rate, created_at) \
         VALUES (?1, ?2, ?3, ?4, ?5) \
         ON CONFLICT(skill_name) DO UPDATE SET \
           prompt = excluded.prompt, \
           expected_outcome = excluded.expected_outcome, \
           pass_rate = excluded.pass_rate, \
           created_at = excluded.created_at",
        params!(
            eval.skill_name,
            eval.prompt,
            eval.expected_outcome,
            eval.pass_rate,
            unix_now()
        ),
    )
    .await?;
    let mut rows = conn
        .query(
            "SELECT id FROM skill_evals WHERE skill_name = ?1",
            params!(eval.skill_name),
        )
        .await?;
    let row = rows
        .next()
        .await?
        .ok_or(crate::error::DbError::NotFound(format!(
            "skill eval for '{}'",
            eval.skill_name
        )))?;
    Ok(row.get(0)?)
}

/// Lists skill evaluations for a skill in insertion order.
///
/// # Errors
///
/// Returns an error when the query fails.
pub async fn list_skill_evals(conn: &Connection, skill_name: &str) -> Result<Vec<SkillEval>> {
    let mut rows = conn
        .query(
            "SELECT id, skill_name, prompt, expected_outcome, pass_rate, created_at \
             FROM skill_evals WHERE skill_name = ?1 ORDER BY id",
            params!(skill_name),
        )
        .await?;
    let mut evals = Vec::new();
    while let Some(row) = rows.next().await? {
        evals.push(SkillEval {
            id: row.get(0)?,
            skill_name: row.get(1)?,
            prompt: row.get(2)?,
            expected_outcome: row.get(3)?,
            pass_rate: row.get(4)?,
            created_at: row.get(5)?,
        });
    }
    Ok(evals)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[tokio::test(flavor = "current_thread")]
    async fn insert_trace_roundtrips_and_lists_by_session() {
        let dir = tempfile::tempdir().unwrap();
        let conn = crate::migrate::connect_and_migrate(dir.path())
            .await
            .unwrap();
        insert_trace(
            &conn,
            &NewTrace {
                task_id: None,
                session_id: "s1",
                command: Some("cargo check"),
                error_diff: Some("E0308"),
                resolution_steps: Some("added lifetime"),
            },
        )
        .await
        .unwrap();

        let traces = list_traces(&conn, "s1").await.unwrap();
        assert_eq!(traces.len(), 1);
        assert_eq!(traces[0].command.as_deref(), Some("cargo check"));
        assert_eq!(traces[0].error_diff.as_deref(), Some("E0308"));
        assert!(list_traces(&conn, "s2").await.unwrap().is_empty());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn get_trace_returns_inserted_trace() {
        let dir = tempfile::tempdir().unwrap();
        let conn = crate::migrate::connect_and_migrate(dir.path())
            .await
            .unwrap();
        let id = insert_trace(
            &conn,
            &NewTrace {
                task_id: None,
                session_id: "s1",
                command: Some("cargo check"),
                error_diff: Some("E0308"),
                resolution_steps: Some("added lifetime"),
            },
        )
        .await
        .unwrap();

        let trace = get_trace(&conn, id).await.unwrap().unwrap();
        assert_eq!(trace.id, id);
        assert_eq!(trace.task_id, None);
        assert_eq!(trace.session_id, "s1");
        assert_eq!(trace.command.as_deref(), Some("cargo check"));
        assert_eq!(trace.error_diff.as_deref(), Some("E0308"));
        assert_eq!(trace.resolution_steps.as_deref(), Some("added lifetime"));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn get_trace_returns_none_for_missing_id() {
        let dir = tempfile::tempdir().unwrap();
        let conn = crate::migrate::connect_and_migrate(dir.path())
            .await
            .unwrap();
        assert!(get_trace(&conn, 999).await.unwrap().is_none());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn insert_heuristic_roundtrips_and_lists_by_skill() {
        let dir = tempfile::tempdir().unwrap();
        let conn = crate::migrate::connect_and_migrate(dir.path())
            .await
            .unwrap();
        let trace_id = insert_trace(
            &conn,
            &NewTrace {
                task_id: None,
                session_id: "s1",
                command: None,
                error_diff: None,
                resolution_steps: None,
            },
        )
        .await
        .unwrap();
        insert_heuristic(
            &conn,
            &NewHeuristic {
                skill_name: "event-modeler",
                pattern: "derive serde before thiserror",
                description: Some("keeps events contract-first"),
                source_trace_id: Some(trace_id),
            },
        )
        .await
        .unwrap();

        let heuristics = list_heuristics(&conn, "event-modeler").await.unwrap();
        assert_eq!(heuristics.len(), 1);
        assert_eq!(heuristics[0].pattern, "derive serde before thiserror");
        assert_eq!(heuristics[0].source_trace_id, Some(trace_id));
        assert!(list_heuristics(&conn, "other").await.unwrap().is_empty());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn insert_skill_eval_roundtrips_with_nullable_fields() {
        let dir = tempfile::tempdir().unwrap();
        let conn = crate::migrate::connect_and_migrate(dir.path())
            .await
            .unwrap();
        insert_skill_eval(
            &conn,
            &NewSkillEval {
                skill_name: "harness",
                prompt: Some("clippy fired; protocol?"),
                expected_outcome: Some("self-correction steps"),
                pass_rate: Some(1.0),
            },
        )
        .await
        .unwrap();

        let evals = list_skill_evals(&conn, "harness").await.unwrap();
        assert_eq!(evals.len(), 1);
        assert_eq!(evals[0].prompt.as_deref(), Some("clippy fired; protocol?"));
        assert_eq!(evals[0].pass_rate, Some(1.0));

        // A later eval for the same skill overwrites rather than appending.
        insert_skill_eval(
            &conn,
            &NewSkillEval {
                skill_name: "harness",
                prompt: Some("later eval"),
                expected_outcome: Some("out"),
                pass_rate: Some(0.5),
            },
        )
        .await
        .unwrap();
        let evals = list_skill_evals(&conn, "harness").await.unwrap();
        assert_eq!(evals.len(), 1);
        assert_eq!(evals[0].prompt.as_deref(), Some("later eval"));
        assert_eq!(evals[0].pass_rate, Some(0.5));
    }
}
