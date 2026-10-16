//! Beat rows: insert, list, prune, and the newest-sensor-beat lookup.
//!
//! Split out of `repo_exec` so the module stays under the LOC ceiling; the
//! scope/status/type vocabulary is [`do_harness_types::BeatScope`],
//! [`do_harness_types::BeatStatus`], and [`do_harness_types::BeatType`].

use std::str::FromStr;

use crate::error::{DbError, Result};
use do_harness_types::{Beat, BeatScope, BeatStatus, BeatType};
use libsql::{Connection, params, params::Params};

/// Insert parameters for a new beat.
#[derive(Debug, Clone)]
pub struct NewBeat<'a> {
    /// Owning task id, when the beat belongs to a task.
    pub task_id: Option<i64>,
    /// Workstream scope. `None` derives `task:<id>` from `task_id` (or
    /// `global` without one); an explicit scope must agree with `task_id`.
    pub scope: Option<BeatScope>,
    /// Beat kind.
    pub beat_type: BeatType,
    /// Outcome label.
    pub status: BeatStatus,
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
    let scope = resolved_scope(beat)?;
    let scope_key = scope.key();
    let mut rows = conn
        .query(
            "INSERT INTO beats (task_id, scope, beat_type, status, sensor_exit_code, sensor_name, \
             started_at, completed_at, input_digest) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9) \
             RETURNING id",
            params!(
                beat.task_id,
                scope_key,
                beat.beat_type.as_str(),
                beat.status.as_str(),
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

/// Resolves the scope column for a beat: an explicit scope must agree with
/// `task_id`, while a missing scope derives from it. The single choke point
/// for the scope contract, so beats and strikes always agree on the key and
/// the `task_id` and `scope` gate lookups always agree with each other.
///
/// # Errors
///
/// Returns [`DbError::BeatScopeMismatch`] when an explicit scope disagrees
/// with `task_id`.
pub(super) fn resolved_scope(beat: &NewBeat<'_>) -> Result<BeatScope> {
    match &beat.scope {
        Some(scope) => {
            scope
                .check_consistency(beat.task_id)
                .map_err(|_| DbError::BeatScopeMismatch {
                    scope: scope.key(),
                    task_id: beat.task_id,
                })?;
            Ok(scope.clone())
        }
        None => Ok(BeatScope::scope_for_task(beat.task_id)),
    }
}

/// Reads one [`Beat`] from a `beats` row, failing closed on stored values
/// outside the beat vocabulary instead of letting them silently miss gates.
///
/// # Errors
///
/// Returns an error when a row value cannot be read or is outside the
/// [`BeatStatus`], [`BeatType`], or [`BeatScope`] vocabulary.
fn beat_from_row(row: &libsql::Row) -> Result<Beat> {
    let beat_type: String = row.get(3)?;
    let status: String = row.get(4)?;
    let scope: String = row.get(2)?;
    Ok(Beat {
        id: row.get(0)?,
        task_id: row.get(1)?,
        scope: BeatScope::from_str(&scope).map_err(|err| DbError::InvalidBeatScope(err.value))?,
        beat_type: BeatType::from_str(&beat_type)
            .map_err(|err| DbError::InvalidBeatType(err.value))?,
        status: BeatStatus::from_str(&status)
            .map_err(|err| DbError::InvalidBeatStatus(err.value))?,
        sensor_exit_code: row.get(5)?,
        sensor_name: row.get(6)?,
        started_at: row.get(7)?,
        completed_at: row.get(8)?,
    })
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
        beats.push(beat_from_row(&row)?);
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
        beats.push(beat_from_row(&row)?);
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

/// The newest sensor beat recorded for one `(sensor, scope)` key.
///
/// Named fields rather than a positional tuple: `status` and `input_digest` are
/// both strings, so the two are easy to transpose silently at a call site.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LatestSensorBeat {
    /// Beat row id, recorded as the reused beat under `--unchanged=skip`.
    pub id: i64,
    /// Outcome of that beat.
    pub status: BeatStatus,
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
/// Returns an error when the query fails or the stored status is outside the
/// beat vocabulary.
pub async fn latest_sensor_beat(
    conn: &Connection,
    sensor_name: &str,
    scope: &str,
) -> Result<Option<LatestSensorBeat>> {
    let mut rows = conn
        .query(
            "SELECT id, status, sensor_exit_code, input_digest FROM beats \
             WHERE beat_type = ?1 AND sensor_name = ?2 AND scope = ?3 \
             ORDER BY id DESC LIMIT 1",
            params!(BeatType::Sensor.as_str(), sensor_name, scope),
        )
        .await?;
    match rows.next().await? {
        Some(row) => {
            let status: String = row.get(1)?;
            Ok(Some(LatestSensorBeat {
                id: row.get(0)?,
                status: BeatStatus::from_str(&status)
                    .map_err(|err| DbError::InvalidBeatStatus(err.value))?,
                exit_code: row.get(2)?,
                input_digest: row.get(3)?,
            }))
        }
        None => Ok(None),
    }
}
