//! Repository layer for execution telemetry: beats and error signatures.

use anyhow::{Context, Result};
use do_harness_types::{Beat, ErrorSignature};
use libsql::{Connection, params, params::Params};

use crate::migrate::unix_now;

/// Insert parameters for a new beat.
#[derive(Debug, Clone)]
pub struct NewBeat<'a> {
    /// Owning task id, when the beat belongs to a task.
    pub task_id: Option<i64>,
    /// Beat kind (e.g. `sensor`).
    pub beat_type: &'a str,
    /// Outcome label (e.g. `ok`, `failed`).
    pub status: &'a str,
    /// Exit code of the sensor that produced this beat.
    pub sensor_exit_code: Option<i32>,
    /// Unix timestamp when the beat started.
    pub started_at: i64,
    /// Unix timestamp when the beat completed.
    pub completed_at: Option<i64>,
}

/// Inserts a beat and returns its id.
///
/// # Errors
///
/// Returns an error when the insert statement fails.
pub async fn insert_beat(conn: &Connection, beat: &NewBeat<'_>) -> Result<i64> {
    conn.execute(
        "INSERT INTO beats (task_id, beat_type, status, sensor_exit_code, started_at, completed_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params!(
            beat.task_id,
            beat.beat_type,
            beat.status,
            beat.sensor_exit_code,
            beat.started_at,
            beat.completed_at
        ),
    )
    .await?;
    Ok(conn.last_insert_rowid())
}

/// Lists beats, optionally filtered to one task.
///
/// # Errors
///
/// Returns an error when the query fails.
pub async fn list_beats(conn: &Connection, task_id: Option<i64>) -> Result<Vec<Beat>> {
    let mut rows = match task_id {
        Some(id) => {
            conn.query(
                "SELECT id, task_id, beat_type, status, sensor_exit_code, started_at, completed_at \
                 FROM beats WHERE task_id = ?1 ORDER BY id",
                params!(id),
            )
            .await?
        }
        None => {
            conn.query(
                "SELECT id, task_id, beat_type, status, sensor_exit_code, started_at, completed_at \
                 FROM beats ORDER BY id",
                Params::None,
            )
            .await?
        }
    };
    let mut beats = Vec::new();
    while let Some(row) = rows.next().await? {
        beats.push(Beat {
            id: row.get(0)?,
            task_id: row.get(1)?,
            beat_type: row.get(2)?,
            status: row.get(3)?,
            sensor_exit_code: row.get(4)?,
            started_at: row.get(5)?,
            completed_at: row.get(6)?,
        });
    }
    Ok(beats)
}

/// Records a new error-signature attempt or increments an existing one.
///
/// `signature` is unique; a fresh signature starts at 1, subsequent calls
/// increment it. Returns the new attempt count.
///
/// # Errors
///
/// Returns an error when the update, insert, or follow-up query fails.
pub async fn bump_error_signature(
    conn: &Connection,
    signature: &str,
    task_id: Option<i64>,
    message: Option<&str>,
) -> Result<i64> {
    let updated = conn
        .execute(
            "UPDATE error_signatures \
             SET attempt_count = attempt_count + 1, \
                 task_id = COALESCE(?1, task_id), \
                 message = COALESCE(?2, message) \
             WHERE signature = ?3",
            params!(task_id, message, signature),
        )
        .await?;
    if updated == 0 {
        conn.execute(
            "INSERT INTO error_signatures (signature, task_id, attempt_count, message, created_at) \
             VALUES (?1, ?2, 1, ?3, ?4)",
            params!(signature, task_id, message, unix_now()),
        )
        .await?;
    }
    let mut rows = conn
        .query(
            "SELECT attempt_count FROM error_signatures WHERE signature = ?1",
            params!(signature),
        )
        .await?;
    let row = rows
        .next()
        .await?
        .context("error signature vanished after bump")?;
    Ok(row.get(0)?)
}

/// Fetches an error signature by its unique key.
///
/// # Errors
///
/// Returns an error when the query fails.
pub async fn get_error_signature(
    conn: &Connection,
    signature: &str,
) -> Result<Option<ErrorSignature>> {
    let mut rows = conn
        .query(
            "SELECT id, signature, task_id, attempt_count, message, created_at \
             FROM error_signatures WHERE signature = ?1",
            params!(signature),
        )
        .await?;
    match rows.next().await? {
        Some(row) => Ok(Some(ErrorSignature {
            id: row.get(0)?,
            signature: row.get(1)?,
            task_id: row.get(2)?,
            attempt_count: row.get(3)?,
            message: row.get(4)?,
            created_at: row.get(5)?,
        })),
        None => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use crate::repo::NewTask;

    #[tokio::test(flavor = "current_thread")]
    async fn insert_beat_roundtrips_and_filters_by_task() {
        let dir = tempfile::tempdir().unwrap();
        let conn = crate::migrate::connect_and_migrate(dir.path())
            .await
            .unwrap();
        let task_id = crate::repo::insert_task(
            &conn,
            &NewTask {
                title: "slice",
                method: Some("vertical-event-slice"),
                subtask_index: 0,
                precondition: None,
                parent_id: None,
            },
        )
        .await
        .unwrap();
        insert_beat(
            &conn,
            &NewBeat {
                task_id: Some(task_id),
                beat_type: "sensor",
                status: "failed",
                sensor_exit_code: Some(1),
                started_at: 1,
                completed_at: Some(2),
            },
        )
        .await
        .unwrap();

        let beats = list_beats(&conn, Some(task_id)).await.unwrap();
        assert_eq!(beats.len(), 1);
        assert_eq!(beats[0].beat_type, "sensor");
        assert_eq!(beats[0].status, "failed");
        assert_eq!(beats[0].sensor_exit_code, Some(1));
        assert_eq!(beats[0].started_at, 1);
        assert!(list_beats(&conn, None).await.unwrap().len() == 1);
        assert!(
            list_beats(&conn, Some(task_id + 1))
                .await
                .unwrap()
                .is_empty()
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn bump_error_signature_starts_at_one_and_increments() {
        let dir = tempfile::tempdir().unwrap();
        let conn = crate::migrate::connect_and_migrate(dir.path())
            .await
            .unwrap();

        assert_eq!(
            bump_error_signature(&conn, "sensor:clippy", None, Some("m1"))
                .await
                .unwrap(),
            1
        );
        assert_eq!(
            bump_error_signature(&conn, "sensor:clippy", None, Some("m2"))
                .await
                .unwrap(),
            2
        );
        assert_eq!(
            bump_error_signature(&conn, "sensor:clippy", None, None)
                .await
                .unwrap(),
            3
        );

        let sig = get_error_signature(&conn, "sensor:clippy")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(sig.attempt_count, 3);
        assert_eq!(sig.message.as_deref(), Some("m2"));
    }
}
