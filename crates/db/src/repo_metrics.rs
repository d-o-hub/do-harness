//! Read-only aggregation queries for the `do-harness metrics` report.

use crate::error::{DbError, Result};
use do_harness_types::{BeatStatus, BeatType};
use libsql::Connection;
use serde::Serialize;

/// Per-sensor beat statistics for the metrics report.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SensorStat {
    /// Sensor name.
    pub name: String,
    /// Total recorded runs.
    pub runs: i64,
    /// Runs that did not end `ok`.
    pub failures: i64,
}

/// Aggregates sensor-beat statistics per sensor name, optionally ignoring
/// beats older than `since` or outside `scope`.
///
/// # Errors
///
/// Returns an error when the query fails.
pub async fn sensor_stats(
    conn: &Connection,
    since: Option<i64>,
    scope: Option<&str>,
) -> Result<Vec<SensorStat>> {
    let mut rows = conn
        .query(
            "SELECT sensor_name, COUNT(*), \
             SUM(CASE WHEN status != ?1 THEN 1 ELSE 0 END) \
             FROM beats WHERE beat_type = ?2 AND sensor_name IS NOT NULL \
             AND (?3 IS NULL OR started_at >= ?3) \
             AND (?4 IS NULL OR scope = ?4) \
             GROUP BY sensor_name ORDER BY sensor_name",
            libsql::params!(
                BeatStatus::Ok.as_str(),
                BeatType::Sensor.as_str(),
                since,
                scope
            ),
        )
        .await?;
    let mut stats = Vec::new();
    while let Some(row) = rows.next().await? {
        stats.push(SensorStat {
            name: row.get(0)?,
            runs: row.get(1)?,
            failures: row.get(2)?,
        });
    }
    Ok(stats)
}

/// One recorded sensor execution, as exported by `do-harness report`.
///
/// A beat keeps its timestamps; when it carries no runner measurement the
/// duration is derived here from them, so every consumer (JSON, Prometheus,
/// dashboards) resolves the duration the same way instead of each re-deriving
/// seconds from two columns.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SensorExecution {
    /// Beat primary key.
    pub id: i64,
    /// Sensor name.
    pub sensor: String,
    /// Workstream scope (`branch:<name>`, `task:<id>`, or `global`).
    pub scope: String,
    /// Outcome label (`ok`, `failed`, `warn`).
    pub status: String,
    /// Exit code recorded for the run, when the sensor started.
    pub exit_code: Option<i32>,
    /// Unix timestamp when the run started.
    ///
    /// For beats recorded by `verify --record` this is the recording time; the
    /// run's actual elapsed time lives in `duration_ms` when it was measured.
    pub started_at: i64,
    /// Unix timestamp when the run completed, when known.
    pub completed_at: Option<i64>,
    /// Wall-clock duration in milliseconds: the value the runner measured and
    /// `verify --record` persisted, derived from the timestamps on legacy rows
    /// that carry no measurement, and unknown for rows stamped only with the
    /// recording time (identical placeholder stamps with no measurement).
    pub duration_ms: Option<i64>,
}

/// Lists sensor beats in a time window, oldest first.
///
/// This is the historical counterpart of [`sensor_stats`]: it keeps every
/// execution instead of aggregating, so a CI dashboard can chart durations and
/// exit codes without querying the database directly. `None` means "no filter"
/// for every argument, and a scope of `all` must be resolved to `None` by the
/// caller (the same convention `metrics` uses).
///
/// # Errors
///
/// Returns an error when the query fails.
pub async fn sensor_executions(
    conn: &Connection,
    since: Option<i64>,
    until: Option<i64>,
    scope: Option<&str>,
    sensor: Option<&str>,
) -> Result<Vec<SensorExecution>> {
    let mut rows = conn
        .query(
            "SELECT id, sensor_name, scope, status, sensor_exit_code, started_at, completed_at, \
             duration_ms \
             FROM beats WHERE beat_type = ?1 AND sensor_name IS NOT NULL \
             AND (?2 IS NULL OR started_at >= ?2) \
             AND (?3 IS NULL OR started_at <= ?3) \
             AND (?4 IS NULL OR scope = ?4) \
             AND (?5 IS NULL OR sensor_name = ?5) \
             ORDER BY started_at, id",
            libsql::params!(BeatType::Sensor.as_str(), since, until, scope, sensor),
        )
        .await?;
    let mut executions = Vec::new();
    while let Some(row) = rows.next().await? {
        let started_at: i64 = row.get(5)?;
        let completed_at: Option<i64> = row.get(6)?;
        let measured: Option<i64> = row.get(7)?;
        executions.push(SensorExecution {
            id: row.get(0)?,
            sensor: row.get(1)?,
            scope: row.get(2)?,
            status: row.get(3)?,
            exit_code: row.get(4)?,
            started_at,
            completed_at,
            duration_ms: measured.or_else(|| {
                completed_at
                    .filter(|completed| *completed != started_at)
                    .map(|completed| completed.saturating_sub(started_at) * 1000)
            }),
        });
    }
    Ok(executions)
}

/// Whether any beat ever ended `ok` (existence probe, not a table scan into
/// the application).
///
/// # Errors
///
/// Returns an error when the query fails.
pub async fn has_ok_beat(conn: &Connection) -> Result<bool> {
    let mut rows = conn
        .query(
            "SELECT EXISTS(SELECT 1 FROM beats WHERE status = ?1)",
            libsql::params!(BeatStatus::Ok.as_str()),
        )
        .await?;
    let row = rows
        .next()
        .await?
        .ok_or_else(|| DbError::Invariant("EXISTS probe returned no row".to_string()))?;
    let exists: i64 = row.get(0)?;
    Ok(exists != 0)
}

#[cfg(test)]
mod tests;
