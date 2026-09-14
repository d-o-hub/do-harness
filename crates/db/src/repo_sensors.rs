//! Repository layer for sensor findings telemetry and bless history.

use crate::error::Result;
use libsql::{Connection, params, params::Params};

/// Highest findings count observed for one sensor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SensorFindings {
    /// Sensor name.
    pub sensor_name: String,
    /// Highest `FINDINGS:` count recorded so far.
    pub max_findings: i64,
    /// Unix timestamp of the last update.
    pub updated_at: i64,
}

/// One append-only baseline bless.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SensorBless {
    /// Sensor name.
    pub sensor_name: String,
    /// Blessed ceiling.
    pub max_findings: i64,
    /// Previous ceiling, when one existed.
    pub previous_max: Option<i64>,
    /// Approver identity recorded at bless time.
    pub approver: String,
    /// Unix timestamp of the bless.
    pub blessed_at: i64,
}

/// Upserts a sensor's observed findings, keeping the running maximum.
///
/// # Errors
///
/// Returns an error when the statement fails.
pub async fn upsert_sensor_findings(
    conn: &Connection,
    sensor_name: &str,
    findings: i64,
    updated_at: i64,
) -> Result<()> {
    conn.execute(
        "INSERT INTO sensor_findings (sensor_name, max_findings, updated_at) \
         VALUES (?1, ?2, ?3) \
         ON CONFLICT(sensor_name) DO UPDATE SET \
             max_findings = MAX(max_findings, excluded.max_findings), \
             updated_at = excluded.updated_at",
        params!(sensor_name, findings, updated_at),
    )
    .await?;
    Ok(())
}

/// Lists observed findings maxima ordered by sensor name.
///
/// # Errors
///
/// Returns an error when the query fails.
pub async fn list_sensor_findings(conn: &Connection) -> Result<Vec<SensorFindings>> {
    let mut rows = conn
        .query(
            "SELECT sensor_name, max_findings, updated_at FROM sensor_findings \
             ORDER BY sensor_name",
            Params::None,
        )
        .await?;
    let mut findings = Vec::new();
    while let Some(row) = rows.next().await? {
        findings.push(SensorFindings {
            sensor_name: row.get(0)?,
            max_findings: row.get(1)?,
            updated_at: row.get(2)?,
        });
    }
    Ok(findings)
}

/// Appends one bless record to the audit history.
///
/// # Errors
///
/// Returns an error when the insert fails.
pub async fn insert_sensor_bless(
    conn: &Connection,
    sensor_name: &str,
    max_findings: i64,
    previous_max: Option<i64>,
    approver: &str,
    blessed_at: i64,
) -> Result<()> {
    conn.execute(
        "INSERT INTO sensor_baseline_blesses \
         (sensor_name, max_findings, previous_max, approver, blessed_at) \
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params!(
            sensor_name,
            max_findings,
            previous_max,
            approver,
            blessed_at
        ),
    )
    .await?;
    Ok(())
}

/// Lists bless history, newest first.
///
/// # Errors
///
/// Returns an error when the query fails.
pub async fn list_sensor_blesses(conn: &Connection) -> Result<Vec<SensorBless>> {
    let mut rows = conn
        .query(
            "SELECT sensor_name, max_findings, previous_max, approver, blessed_at \
             FROM sensor_baseline_blesses ORDER BY id DESC",
            Params::None,
        )
        .await?;
    let mut blesses = Vec::new();
    while let Some(row) = rows.next().await? {
        blesses.push(SensorBless {
            sensor_name: row.get(0)?,
            max_findings: row.get(1)?,
            previous_max: row.get(2)?,
            approver: row.get(3)?,
            blessed_at: row.get(4)?,
        });
    }
    Ok(blesses)
}
