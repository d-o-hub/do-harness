//! Repository layer for execution telemetry: error signatures.
//!
//! Beat rows (insert/list/prune/latest) live in the [`beats`] submodule and are
//! re-exported here so every existing `repo_exec::<item>` path stays valid.

use std::str::FromStr;

use crate::error::{DbError, Result};
use crate::migrate::unix_now;
use crate::repo_scope::reset_error_signature;
use do_harness_types::{BeatScope, ErrorSignature};
use libsql::{Connection, params};

mod beats;
pub use beats::*;

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
        let task_id = BeatScope::from_str(scope)
            .map_err(|err| DbError::InvalidBeatScope(err.value))?
            .task_id();
        conn.execute(
            "INSERT INTO error_signatures \
             (signature, scope, task_id, attempt_count, message, created_at) \
             VALUES (?1, ?2, ?3, 1, ?4, ?5)",
            params!(signature, scope, task_id, message, unix_now()),
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
    let scope = resolved_scope(beat)?.key();
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
        let scope = resolved_scope(&outcome.beat)?.key();
        if outcome.ok {
            reset_error_signature(&tx, &signature, &scope).await?;
        } else {
            bump_error_signature_on(&tx, &signature, &scope, outcome.message).await?;
        }
    }
    tx.commit().await?;
    Ok(())
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
