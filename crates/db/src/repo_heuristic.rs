//! Repository layer for the `heuristics` table.

use crate::error::{DbError, Result};
use crate::migrate::unix_now;
use do_harness_types::Heuristic;
use libsql::{Connection, params};

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

/// Inserts a heuristic and returns its id.
///
/// # Errors
///
/// Returns an error when the insert statement fails.
pub async fn insert_heuristic(conn: &Connection, heuristic: &NewHeuristic<'_>) -> Result<i64> {
    let mut rows = conn
        .query(
            "INSERT INTO heuristics (skill_name, pattern, description, source_trace_id, created_at) \
             VALUES (?1, ?2, ?3, ?4, ?5) \
             RETURNING id",
            params!(
                heuristic.skill_name,
                heuristic.pattern,
                heuristic.description,
                heuristic.source_trace_id,
                unix_now()
            ),
        )
        .await?;
    let row = rows
        .next()
        .await?
        .ok_or_else(|| DbError::Invariant("heuristic id vanished after insert".to_string()))?;
    Ok(row.get(0)?)
}

/// Skill-filtered read behind [`list_heuristics`], shared with its query-plan
/// test so the plan under test is the plan that ships. Migration `0023` adds
/// the matching index.
const LIST_BY_SKILL_SQL: &str = "SELECT id, skill_name, pattern, description, source_trace_id, \
     created_at FROM heuristics WHERE skill_name = ?1 ORDER BY id";

/// Lists heuristics for a skill in insertion order.
///
/// # Errors
///
/// Returns an error when the query fails.
pub async fn list_heuristics(conn: &Connection, skill_name: &str) -> Result<Vec<Heuristic>> {
    let mut rows = conn.query(LIST_BY_SKILL_SQL, params!(skill_name)).await?;
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

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use crate::repo_trace::{NewTrace, insert_trace};

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
        assert_eq!(list_heuristics(&conn, "other").await.unwrap().len(), 0);
    }

    /// The skill-filtered read runs on the `0023` index instead of scanning the
    /// table, and the same index serves the `ORDER BY id`: `SQLite` appends the
    /// rowid to every index entry of a rowid table, so one indexed column is
    /// enough. Dropping the index from the migration fails this test.
    #[tokio::test(flavor = "current_thread")]
    async fn skill_filtered_read_uses_the_skill_index() {
        let dir = tempfile::tempdir().unwrap();
        let conn = crate::migrate::connect_and_migrate(dir.path())
            .await
            .unwrap();
        for _ in 0..8 {
            insert_heuristic(
                &conn,
                &NewHeuristic {
                    skill_name: "probe",
                    pattern: "p",
                    description: None,
                    source_trace_id: None,
                },
            )
            .await
            .unwrap();
        }

        let mut rows = conn
            .query(
                &format!("EXPLAIN QUERY PLAN {LIST_BY_SKILL_SQL}"),
                params!("probe"),
            )
            .await
            .unwrap();
        let mut plan = Vec::new();
        while let Some(row) = rows.next().await.unwrap() {
            plan.push(row.get::<String>(3).unwrap());
        }
        let plan = plan.join("\n");
        assert!(
            plan.contains("idx_heuristics_skill_name"),
            "the skill read must use the index, got:\n{plan}"
        );
        assert!(
            !plan.contains("TEMP B-TREE"),
            "ORDER BY id must come from the index, got:\n{plan}"
        );
    }
}
