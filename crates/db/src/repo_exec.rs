//! Repository layer for execution telemetry: beats and error signatures.

use crate::error::{DbError, Result};
use crate::migrate::unix_now;
use crate::repo_scope::reset_error_signature;
use do_harness_types::{Beat, ErrorSignature};
use libsql::{Connection, params, params::Params};

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
    /// Name of the sensor that produced this beat, when it is a sensor beat.
    pub sensor_name: Option<&'a str>,
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
    let mut rows = conn
        .query(
            "INSERT INTO beats (task_id, beat_type, status, sensor_exit_code, sensor_name, \
             started_at, completed_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7) \
             RETURNING id",
            params!(
                beat.task_id,
                beat.beat_type,
                beat.status,
                beat.sensor_exit_code,
                beat.sensor_name,
                beat.started_at,
                beat.completed_at
            ),
        )
        .await?;
    let row = rows
        .next()
        .await?
        .ok_or_else(|| DbError::NotFound("beat id vanished after insert".to_string()))?;
    Ok(row.get(0)?)
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
                "SELECT id, task_id, beat_type, status, sensor_exit_code, sensor_name, \
                 started_at, completed_at FROM beats WHERE task_id = ?1 ORDER BY id",
                params!(id),
            )
            .await?
        }
        None => {
            conn.query(
                "SELECT id, task_id, beat_type, status, sensor_exit_code, sensor_name, \
                 started_at, completed_at FROM beats ORDER BY id",
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
            sensor_name: row.get(5)?,
            started_at: row.get(6)?,
            completed_at: row.get(7)?,
        });
    }
    Ok(beats)
}

/// Records a new error-signature attempt or increments an existing one,
/// scoped by `(signature, task_id)`.
///
/// The pair is unique; a fresh pair starts at 1, subsequent calls increment
/// it. `task_id = None` scopes the signature to the whole workspace. Returns
/// the new attempt count. The update-or-insert sequence runs in a transaction
/// so concurrent bumps cannot race between the `UPDATE` and the `INSERT`.
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
    let tx = conn.transaction().await?;
    let count = bump_error_signature_on(&tx, signature, task_id, message).await?;
    tx.commit().await?;
    Ok(count)
}

/// [`bump_error_signature`] without transaction management, for composing
/// into a larger transaction (see [`record_sensor_outcome`]).
async fn bump_error_signature_on(
    conn: &Connection,
    signature: &str,
    task_id: Option<i64>,
    message: Option<&str>,
) -> Result<i64> {
    let updated = conn
        .execute(
            "UPDATE error_signatures \
             SET attempt_count = attempt_count + 1, \
                 message = COALESCE(?1, message) \
             WHERE signature = ?2 AND task_id IS ?3",
            params!(message, signature, task_id),
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
            "SELECT attempt_count FROM error_signatures WHERE signature = ?1 AND task_id IS ?2",
            params!(signature, task_id),
        )
        .await?;
    let row = rows
        .next()
        .await?
        .ok_or_else(|| DbError::NotFound("error signature vanished after bump".to_string()))?;
    Ok(row.get(0)?)
}

/// Records one sensor outcome atomically: inserts the beat and resets (on
/// success) or bumps (on failure) the matching `sensor:<name>` signature in a
/// single transaction, so a crash or interleaved writer can never persist a
/// beat whose strike counter did not move with it.
///
/// Returns the resulting strike count for the sensor's signature.
///
/// # Errors
///
/// Returns an error when the state database cannot be written.
pub async fn record_sensor_outcome(
    conn: &Connection,
    beat: &NewBeat<'_>,
    ok: bool,
    message: Option<&str>,
) -> Result<i64> {
    let signature = format!("sensor:{}", beat.sensor_name.unwrap_or("unknown"));
    let tx = conn.transaction().await?;
    insert_beat(&tx, beat).await?;
    let count = if ok {
        reset_error_signature(&tx, &signature, beat.task_id).await?;
        0
    } else {
        bump_error_signature_on(&tx, &signature, beat.task_id, message).await?
    };
    tx.commit().await?;
    Ok(count)
}

/// Fetches an error signature by its `(signature, task_id)` key.
///
/// # Errors
///
/// Returns an error when the query fails.
pub async fn get_error_signature(
    conn: &Connection,
    signature: &str,
    task_id: Option<i64>,
) -> Result<Option<ErrorSignature>> {
    let mut rows = conn
        .query(
            "SELECT id, signature, task_id, attempt_count, message, created_at \
             FROM error_signatures WHERE signature = ?1 AND task_id IS ?2",
            params!(signature, task_id),
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
    #![allow(clippy::unwrap_used, clippy::expect_used)]

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
                sensor_name: Some("check"),
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
        assert_eq!(list_beats(&conn, None).await.unwrap().len(), 1);
        assert!(
            list_beats(&conn, Some(task_id + 1))
                .await
                .unwrap()
                .is_empty()
        );
    }

    /// Foreign keys are enforced: a beat referencing a missing task is
    /// rejected instead of silently orphaned.
    #[tokio::test(flavor = "current_thread")]
    async fn insert_beat_rejects_missing_task_fk() {
        let dir = tempfile::tempdir().unwrap();
        let conn = crate::migrate::connect_and_migrate(dir.path())
            .await
            .unwrap();
        let result = insert_beat(
            &conn,
            &NewBeat {
                task_id: Some(9999),
                beat_type: "sensor",
                status: "ok",
                sensor_exit_code: Some(0),
                sensor_name: Some("check"),
                started_at: 1,
                completed_at: Some(2),
            },
        )
        .await;
        assert!(result.is_err(), "FK violation must surface as an error");
    }

    /// Workspace-global strikes (NULL `task_id`) are unique per signature: a
    /// raw duplicate insert violates the partial unique index.
    #[tokio::test(flavor = "current_thread")]
    async fn duplicate_global_signature_insert_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let conn = crate::migrate::connect_and_migrate(dir.path())
            .await
            .unwrap();
        bump_error_signature(&conn, "sensor:clippy", None, Some("m1"))
            .await
            .unwrap();
        let dupe = conn
            .execute(
                "INSERT INTO error_signatures (signature, task_id, attempt_count, message, \
                 created_at) VALUES ('sensor:clippy', NULL, 1, NULL, 0)",
                Params::None,
            )
            .await;
        assert!(dupe.is_err(), "duplicate global strike must be rejected");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn record_sensor_outcome_is_atomic_beat_plus_strike() {
        let dir = tempfile::tempdir().unwrap();
        let conn = crate::migrate::connect_and_migrate(dir.path())
            .await
            .unwrap();

        let beat = |status: &'static str| NewBeat {
            task_id: None,
            beat_type: "sensor",
            status,
            sensor_exit_code: Some(0),
            sensor_name: Some("atomic"),
            started_at: 1,
            completed_at: Some(2),
        };
        record_sensor_outcome(&conn, &beat("failed"), false, Some("boom"))
            .await
            .unwrap();
        record_sensor_outcome(&conn, &beat("failed"), false, Some("boom2"))
            .await
            .unwrap();
        assert_eq!(
            get_error_signature(&conn, "sensor:atomic", None)
                .await
                .unwrap()
                .unwrap()
                .attempt_count,
            2
        );
        record_sensor_outcome(&conn, &beat("ok"), true, None)
            .await
            .unwrap();
        assert!(
            get_error_signature(&conn, "sensor:atomic", None)
                .await
                .unwrap()
                .is_none(),
            "passing outcome must reset the strike inside the same transaction"
        );
        assert_eq!(list_beats(&conn, None).await.unwrap().len(), 3);
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

        let sig = get_error_signature(&conn, "sensor:clippy", None)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(sig.attempt_count, 3);
        assert_eq!(sig.message.as_deref(), Some("m2"));
    }
}
