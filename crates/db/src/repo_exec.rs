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
    /// Workstream scope: `task:<id>`, `branch:<name>`, or `global`.
    pub scope: Option<&'a str>,
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
/// The unchanged-input digest is not part of a beat's identity: only the verify
/// batch that records one uses [`insert_beat_with_digest`], and every other
/// caller writes the column as `NULL`.
///
/// # Errors
///
/// Returns an error when the insert statement fails.
pub(crate) async fn insert_beat(conn: &Connection, beat: &NewBeat<'_>) -> Result<i64> {
    insert_beat_with_digest(conn, beat, None).await
}

/// Inserts a beat with its unchanged-input digest in the same statement.
///
/// Binding the digest here, rather than following the insert with an `UPDATE`,
/// means a beat row never exists without the digest of the inputs it was
/// recorded against.
///
/// # Errors
///
/// Returns an error when the insert statement fails.
pub(crate) async fn insert_beat_with_digest(
    conn: &Connection,
    beat: &NewBeat<'_>,
    input_digest: Option<&str>,
) -> Result<i64> {
    let task_scope_buf;
    let scope_str = match beat.scope {
        Some(s) => s,
        None => match beat.task_id {
            Some(task_id) => {
                task_scope_buf = format!("task:{task_id}");
                &task_scope_buf
            }
            None => "global",
        },
    };
    let mut rows = conn
        .query(
            "INSERT INTO beats (task_id, scope, beat_type, status, sensor_exit_code, sensor_name, \
             started_at, completed_at, input_digest) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9) \
             RETURNING id",
            params!(
                beat.task_id,
                scope_str,
                beat.beat_type,
                beat.status,
                beat.sensor_exit_code,
                beat.sensor_name,
                beat.started_at,
                beat.completed_at,
                input_digest
            ),
        )
        .await?;
    let row = rows
        .next()
        .await?
        .ok_or_else(|| DbError::Invariant("beat id vanished after insert".to_string()))?;
    Ok(row.get(0)?)
}

/// Derives the workstream scope of a beat, mirroring [`insert_beat`]'s scope
/// column so beats and strikes always agree on the key.
fn beat_scope(beat: &NewBeat<'_>) -> String {
    match beat.scope {
        Some(scope) => scope.to_owned(),
        None => match beat.task_id {
            Some(id) => format!("task:{id}"),
            None => "global".to_owned(),
        },
    }
}

/// Task id encoded by a scope key, kept for the legacy `task_id` column.
fn scope_task_id(scope: &str) -> Option<i64> {
    scope.strip_prefix("task:").and_then(|id| id.parse().ok())
}

/// Lists beats, optionally filtered to one task.
///
/// # Errors
///
/// Returns an error when the query fails.
pub async fn list_beats(conn: &Connection, task_id: Option<i64>) -> Result<Vec<Beat>> {
    list_beats_page(conn, task_id, None, 0).await
}

/// Lists beats with `LIMIT`/`OFFSET` paging (`limit: None` returns every row).
///
/// # Errors
///
/// Returns an error when the query fails.
pub async fn list_beats_page(
    conn: &Connection,
    task_id: Option<i64>,
    limit: Option<u32>,
    offset: u32,
) -> Result<Vec<Beat>> {
    let mut rows = match task_id {
        Some(id) => {
            conn.query(
                "SELECT id, task_id, scope, beat_type, status, sensor_exit_code, sensor_name, \
                 started_at, completed_at FROM beats WHERE task_id = ?1 ORDER BY id \
                 LIMIT ?2 OFFSET ?3",
                params!(id, crate::bind_limit(limit), i64::from(offset)),
            )
            .await?
        }
        None => {
            conn.query(
                "SELECT id, task_id, scope, beat_type, status, sensor_exit_code, sensor_name, \
                 started_at, completed_at FROM beats ORDER BY id LIMIT ?1 OFFSET ?2",
                params!(crate::bind_limit(limit), i64::from(offset)),
            )
            .await?
        }
    };
    let mut beats = Vec::new();
    while let Some(row) = rows.next().await? {
        beats.push(Beat {
            id: row.get(0)?,
            task_id: row.get(1)?,
            scope: row.get(2)?,
            beat_type: row.get(3)?,
            status: row.get(4)?,
            sensor_exit_code: row.get(5)?,
            sensor_name: row.get(6)?,
            started_at: row.get(7)?,
            completed_at: row.get(8)?,
        });
    }
    Ok(beats)
}

/// Lists beats scoped to `scope` (e.g. `branch:main`, `task:1`, `global`).
///
/// # Errors
///
/// Returns an error when the query fails.
pub async fn list_beats_by_scope(conn: &Connection, scope: &str) -> Result<Vec<Beat>> {
    let mut rows = conn
        .query(
            "SELECT id, task_id, scope, beat_type, status, sensor_exit_code, sensor_name, \
             started_at, completed_at FROM beats WHERE scope = ?1 ORDER BY id",
            params!(scope),
        )
        .await?;
    let mut beats = Vec::new();
    while let Some(row) = rows.next().await? {
        beats.push(Beat {
            id: row.get(0)?,
            task_id: row.get(1)?,
            scope: row.get(2)?,
            beat_type: row.get(3)?,
            status: row.get(4)?,
            sensor_exit_code: row.get(5)?,
            sensor_name: row.get(6)?,
            started_at: row.get(7)?,
            completed_at: row.get(8)?,
        });
    }
    Ok(beats)
}

/// Deletes beats older than `older_than` while keeping at least
/// `keep_per_task` most-recent beats per task or workstream scope (task-less
/// beats are partitioned by their `scope`, falling back to `global`, so one
/// branch's history cannot evict another's). Returns the number of deleted
/// rows.
///
/// # Errors
///
/// Returns an error when the delete fails.
pub async fn prune_beats(conn: &Connection, older_than: i64, keep_per_task: i64) -> Result<u64> {
    let deleted = conn
        .execute(
            "DELETE FROM beats WHERE started_at < ?1 AND id NOT IN (\
               SELECT id FROM (\
                 SELECT id, ROW_NUMBER() OVER (\
                   PARTITION BY COALESCE(task_id, -1), COALESCE(scope, 'global') ORDER BY id DESC\
                 ) AS rn \
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
/// scoped by `(signature, scope)` where `scope` is the same workstream key as
/// beats (`task:<id>`, `branch:<name>`, or `global`).
///
/// The pair is unique; a fresh pair starts at 1, subsequent calls increment
/// it. Returns the new attempt count. The update-or-insert sequence runs in a
/// transaction so concurrent bumps cannot race between the `UPDATE` and the
/// `INSERT`.
///
/// # Errors
///
/// Returns an error when the update, insert, or follow-up query fails.
pub async fn bump_error_signature(
    conn: &Connection,
    signature: &str,
    scope: &str,
    message: Option<&str>,
) -> Result<i64> {
    let tx = crate::tx::begin_immediate(conn).await?;
    let count = bump_error_signature_on(&tx, signature, scope, message).await?;
    tx.commit().await?;
    Ok(count)
}

/// [`bump_error_signature`] without transaction management, for composing
/// into a larger transaction (see [`record_sensor_outcome`]).
async fn bump_error_signature_on(
    conn: &Connection,
    signature: &str,
    scope: &str,
    message: Option<&str>,
) -> Result<i64> {
    let updated = conn
        .execute(
            "UPDATE error_signatures \
             SET attempt_count = attempt_count + 1, \
                 message = COALESCE(?1, message) \
             WHERE signature = ?2 AND scope = ?3",
            params!(message, signature, scope),
        )
        .await?;
    if updated == 0 {
        conn.execute(
            "INSERT INTO error_signatures \
             (signature, scope, task_id, attempt_count, message, created_at) \
             VALUES (?1, ?2, ?3, 1, ?4, ?5)",
            params!(signature, scope, scope_task_id(scope), message, unix_now()),
        )
        .await?;
    }
    let mut rows = conn
        .query(
            "SELECT attempt_count FROM error_signatures WHERE signature = ?1 AND scope = ?2",
            params!(signature, scope),
        )
        .await?;
    let row = rows
        .next()
        .await?
        .ok_or_else(|| DbError::Invariant("error signature vanished after bump".to_string()))?;
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
    let scope = beat_scope(beat);
    let tx = crate::tx::begin_immediate(conn).await?;
    insert_beat(&tx, beat).await?;
    let count = if ok {
        reset_error_signature(&tx, &signature, &scope).await?;
        0
    } else {
        bump_error_signature_on(&tx, &signature, &scope, message).await?
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
    /// Identity of the sensor's declared inputs at a clean pass, when the
    /// sensor opts into unchanged-input reuse.
    pub input_digest: Option<&'a str>,
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
    let tx = crate::tx::begin_immediate(conn).await?;
    for outcome in outcomes {
        insert_beat_with_digest(&tx, &outcome.beat, outcome.input_digest).await?;
        let signature = format!("sensor:{}", outcome.beat.sensor_name.unwrap_or("unknown"));
        let scope = beat_scope(&outcome.beat);
        if outcome.ok {
            reset_error_signature(&tx, &signature, &scope).await?;
        } else {
            bump_error_signature_on(&tx, &signature, &scope, outcome.message).await?;
        }
    }
    tx.commit().await?;
    Ok(())
}

/// The newest sensor beat recorded for one `(sensor, scope)` key.
///
/// Named fields rather than a positional tuple: `status` and `input_digest` are
/// both strings, so the two are easy to transpose silently at a call site.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LatestSensorBeat {
    /// Beat row id, recorded as the reused beat under `--unchanged=skip`.
    pub id: i64,
    /// Status of that beat (`ok`, `failed`, ...).
    pub status: String,
    /// Exit code the sensor reported, when it reported one.
    pub exit_code: Option<i32>,
    /// Digest of the sensor's declared inputs at a clean pass.
    pub input_digest: Option<String>,
}

/// Returns the most recent sensor beat for `sensor_name` in `scope`
/// (`branch:<name>`, `task:<id>`, or `global`).
///
/// A later failed or warned beat always shadows an earlier pass, so reuse can
/// never search past the newest outcome. The lookup is exact-scope: a beat
/// recorded on one branch never satisfies a query for another.
///
/// # Errors
///
/// Returns an error when the query fails.
pub async fn latest_sensor_beat(
    conn: &Connection,
    sensor_name: &str,
    scope: &str,
) -> Result<Option<LatestSensorBeat>> {
    let mut rows = conn
        .query(
            "SELECT id, status, sensor_exit_code, input_digest FROM beats \
             WHERE beat_type = 'sensor' AND sensor_name = ?1 AND scope = ?2 \
             ORDER BY id DESC LIMIT 1",
            params!(sensor_name, scope),
        )
        .await?;
    match rows.next().await? {
        Some(row) => Ok(Some(LatestSensorBeat {
            id: row.get(0)?,
            status: row.get(1)?,
            exit_code: row.get(2)?,
            input_digest: row.get(3)?,
        })),
        None => Ok(None),
    }
}

/// Fetches an error signature by its `(signature, scope)` key.
///
/// # Errors
///
/// Returns an error when the query fails.
pub async fn get_error_signature(
    conn: &Connection,
    signature: &str,
    scope: &str,
) -> Result<Option<ErrorSignature>> {
    let mut rows = conn
        .query(
            "SELECT id, signature, scope, task_id, attempt_count, message, created_at \
             FROM error_signatures WHERE signature = ?1 AND scope = ?2",
            params!(signature, scope),
        )
        .await?;
    match rows.next().await? {
        Some(row) => Ok(Some(ErrorSignature {
            id: row.get(0)?,
            signature: row.get(1)?,
            scope: row.get(2)?,
            task_id: row.get(3)?,
            attempt_count: row.get(4)?,
            message: row.get(5)?,
            created_at: row.get(6)?,
        })),
        None => Ok(None),
    }
}

/// Returns the error signatures at `scope` whose `attempt_count` meets
/// `threshold` and whose key starts with `prefix`.
///
/// One query replaces a per-sensor lookup loop, so the fail-fast pre-check
/// stays a single round trip as the configured sensor count grows.
///
/// `prefix` is matched literally: `_` and `%` are LIKE wildcards, and a sensor
/// key such as `sensor:cargo_check` would otherwise also strike
/// `sensor:cargoCheckX`, quarantining a sensor that never failed.
///
/// # Errors
///
/// Returns an error when the query fails.
pub async fn struck_signatures(
    conn: &Connection,
    scope: &str,
    prefix: &str,
    threshold: i64,
) -> Result<Vec<String>> {
    let escaped = prefix
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_");
    let mut rows = conn
        .query(
            "SELECT signature FROM error_signatures \
             WHERE scope = ?1 AND attempt_count >= ?2 AND signature LIKE ?3 ESCAPE '\\' \
             ORDER BY signature",
            params!(scope, threshold, format!("{escaped}%")),
        )
        .await?;
    let mut signatures = Vec::new();
    while let Some(row) = rows.next().await? {
        signatures.push(row.get(0)?);
    }
    Ok(signatures)
}

#[cfg(test)]
mod tests;
