//! Read-only aggregation queries for the `do-harness metrics` report.

use crate::error::{DbError, Result};
use libsql::{Connection, params::Params};
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
/// beats older than `since` (Unix seconds; [`None`] means all history).
///
/// # Errors
///
/// Returns an error when the query fails.
pub async fn sensor_stats(conn: &Connection, since: Option<i64>) -> Result<Vec<SensorStat>> {
    let mut rows = conn
        .query(
            "SELECT sensor_name, COUNT(*), \
             SUM(CASE WHEN status != 'ok' THEN 1 ELSE 0 END) \
             FROM beats WHERE beat_type = 'sensor' AND sensor_name IS NOT NULL \
             AND (?1 IS NULL OR started_at >= ?1) \
             GROUP BY sensor_name ORDER BY sensor_name",
            libsql::params!(since),
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

/// Whether any beat ever ended `ok` (existence probe, not a table scan into
/// the application).
///
/// # Errors
///
/// Returns an error when the query fails.
pub async fn has_ok_beat(conn: &Connection) -> Result<bool> {
    let mut rows = conn
        .query(
            "SELECT EXISTS(SELECT 1 FROM beats WHERE status = 'ok')",
            Params::None,
        )
        .await?;
    let row = rows
        .next()
        .await?
        .ok_or_else(|| DbError::NotFound("EXISTS probe returned no row".to_string()))?;
    let exists: i64 = row.get(0)?;
    Ok(exists != 0)
}
