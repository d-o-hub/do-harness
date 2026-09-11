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
pub(crate) async fn insert_beat(conn: &Connection, beat: &NewBeat<'_>) -> Result<i64> {
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
    list_beats_page(conn, task_id, -1, 0).await
}

/// Lists beats with `LIMIT`/`OFFSET` paging (`limit = -1` disables the limit).
///
/// # Errors
///
/// Returns an error when the query fails.
pub async fn list_beats_page(
    conn: &Connection,
    task_id: Option<i64>,
    limit: i64,
    offset: i64,
) -> Result<Vec<Beat>> {
    let mut rows = match task_id {
        Some(id) => {
            conn.query(
                "SELECT id, task_id, beat_type, status, sensor_exit_code, sensor_name, \
                 started_at, completed_at FROM beats WHERE task_id = ?1 ORDER BY id \
                 LIMIT ?2 OFFSET ?3",
                params!(id, limit, offset),
            )
            .await?
        }
        None => {
            conn.query(
                "SELECT id, task_id, beat_type, status, sensor_exit_code, sensor_name, \
                 started_at, completed_at FROM beats ORDER BY id LIMIT ?1 OFFSET ?2",
                params!(limit, offset),
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

/// Deletes beats older than `older_than` while keeping at least
/// `keep_per_task` most-recent beats per task (task-less beats form their own
/// partition). Returns the number of deleted rows.
///
/// # Errors
///
/// Returns an error when the delete fails.
pub async fn prune_beats(conn: &Connection, older_than: i64, keep_per_task: i64) -> Result<u64> {
    let deleted = conn
        .execute(
            "DELETE FROM beats WHERE started_at < ?1 AND id NOT IN (\
               SELECT id FROM (\
                 SELECT id, ROW_NUMBER() OVER (PARTITION BY task_id ORDER BY id DESC) AS rn \
                 FROM beats\
               ) WHERE rn <= ?2\
             )",
            params!(older_than, keep_per_task),
        )
        .await?;
    Ok(deleted)
}

/// Compacts the database file after pruning (`VACUUM`).
///
/// # Errors
///
/// Returns an error when `VACUUM` fails (for example inside a transaction).
pub async fn vacuum(conn: &Connection) -> Result<()> {
    conn.execute("VACUUM", Params::None).await?;
    Ok(())
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

/// One sensor outcome persisted by [`record_verify_batch`].
#[derive(Debug, Clone)]
pub struct SensorOutcome<'a> {
    /// Beat row to insert for the sensor.
    pub beat: NewBeat<'a>,
    /// Whether the sensor passed; failures bump the matching signature.
    pub ok: bool,
    /// Failure output kept as the signature message.
    pub message: Option<&'a str>,
}

/// Persists every sensor outcome in one transaction, so a crash can never
/// leave a partially recorded verify run (half the beats and strikes of a
/// report). Halted sensors are filtered by the caller before batching.
///
/// # Errors
///
/// Returns an error when the transaction, a beat insert, or a signature
/// update fails; nothing is committed unless every outcome succeeds.
pub async fn record_verify_batch(conn: &Connection, outcomes: &[SensorOutcome<'_>]) -> Result<()> {
    const MAX_BUSY_ATTEMPTS: usize = 3;

    crate::error::retry_on_busy(MAX_BUSY_ATTEMPTS, move || {
        let conn = conn;
        let outcomes = outcomes;
        async move { record_verify_batch_once(conn, outcomes).await }
    })
    .await
}

/// Transaction body for [`record_verify_batch`], retried as a unit on busy.
async fn record_verify_batch_once(conn: &Connection, outcomes: &[SensorOutcome<'_>]) -> Result<()> {
    let tx = conn.transaction().await?;
    for outcome in outcomes {
        insert_beat(&tx, &outcome.beat).await?;
        let signature = format!("sensor:{}", outcome.beat.sensor_name.unwrap_or("unknown"));
        if outcome.ok {
            reset_error_signature(&tx, &signature, outcome.beat.task_id).await?;
        } else {
            bump_error_signature_on(&tx, &signature, outcome.beat.task_id, outcome.message).await?;
        }
    }
    tx.commit().await?;
    Ok(())
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
mod tests;
