//! Repository layer for the `skill_evals` table (latest eval per skill).

use crate::error::{DbError, Result};
use crate::migrate::unix_now;
use do_harness_types::SkillEval;
use libsql::{Connection, params, params::Params};

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

/// Inserts or upserts a skill evaluation row and returns its id.
///
/// `skill_name` is unique; a later evaluation for the same skill overwrites the
/// previous one (latest wins), so the table holds one row per skill.
///
/// # Errors
///
/// Returns an error when the insert statement fails.
pub async fn insert_skill_eval(conn: &Connection, eval: &NewSkillEval<'_>) -> Result<i64> {
    let mut rows = conn
        .query(
            "INSERT INTO skill_evals (skill_name, prompt, expected_outcome, pass_rate, created_at) \
             VALUES (?1, ?2, ?3, ?4, ?5) \
             ON CONFLICT(skill_name) DO UPDATE SET \
               prompt = excluded.prompt, \
               expected_outcome = excluded.expected_outcome, \
               pass_rate = excluded.pass_rate, \
               created_at = excluded.created_at \
             RETURNING id",
            params!(
                eval.skill_name,
                eval.prompt,
                eval.expected_outcome,
                eval.pass_rate,
                unix_now()
            ),
        )
        .await?;
    let row = rows.next().await?.ok_or(DbError::NotFound(format!(
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

/// Lists every persisted skill evaluation, one row per skill.
///
/// # Errors
///
/// Returns an error when the query fails.
pub async fn list_all_skill_evals(conn: &Connection) -> Result<Vec<SkillEval>> {
    let mut rows = conn
        .query(
            "SELECT id, skill_name, prompt, expected_outcome, pass_rate, created_at \
             FROM skill_evals ORDER BY skill_name",
            Params::None,
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
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

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
