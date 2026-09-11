//! Repository layer for the `traces` table.

use crate::error::{DbError, Result};
use crate::migrate::unix_now;
use do_harness_types::Trace;
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

/// Inserts a trace and returns its id.
///
/// # Errors
///
/// Returns an error when the insert statement fails.
pub async fn insert_trace(conn: &Connection, trace: &NewTrace<'_>) -> Result<i64> {
    let mut rows = conn
        .query(
            "INSERT INTO traces (task_id, session_id, command, error_diff, resolution_steps, created_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6) \
             RETURNING id",
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
    let row = rows
        .next()
        .await?
        .ok_or_else(|| DbError::NotFound("trace id vanished after insert".to_string()))?;
    Ok(row.get(0)?)
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

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

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
}
