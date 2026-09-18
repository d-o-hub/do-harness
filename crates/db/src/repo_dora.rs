//! Repository layer for the `dora_snapshots` table.

use crate::error::{DbError, Result};
use crate::migrate::unix_now;
use libsql::{Connection, params};

/// Insert parameters for a derived DORA snapshot.
///
/// Every measured quantity is an integer; the change failure rate is kept as
/// an exact `deploys_failed`/`deploy_count` pair, and `derivation` is the
/// caller-serialized JSON manifest that makes the row auditable.
#[derive(Debug, Clone)]
pub struct NewDoraSnapshot<'a> {
    /// Where the numbers came from: `git` or `gh`.
    pub source: &'a str,
    /// Window length in days used for the measurement.
    pub window_days: i64,
    /// Inclusive start of the resolved window.
    pub window_start: i64,
    /// End of the resolved window.
    pub window_end: i64,
    /// `HEAD` revision at measurement time.
    pub source_rev: &'a str,
    /// Deploys measured in the window.
    pub deploy_count: i64,
    /// Deploys whose failure range contained at least one revert.
    pub deploys_failed: i64,
    /// Lead-time samples behind the percentiles.
    pub lead_samples: i64,
    /// Median lead time in seconds, `None` with no samples.
    pub lead_p50_seconds: Option<i64>,
    /// 90th percentile lead time in seconds, `None` with no samples.
    pub lead_p90_seconds: Option<i64>,
    /// Median restore time over restored incidents, `None` when none.
    pub mttr_seconds: Option<i64>,
    /// Incidents with a restoring deploy.
    pub mttr_restored: i64,
    /// Incidents still unrestored.
    pub mttr_unrestored: i64,
    /// Threshold breach count (`FINDINGS:` marker value).
    pub breach_count: i64,
    /// `sha256:`-prefixed digest of the pinned policy.
    pub policy_fingerprint: &'a str,
    /// Serialized derivation manifest.
    pub derivation: &'a str,
}

/// One stored DORA snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DoraSnapshotRow {
    /// Row id.
    pub id: i64,
    /// Unix timestamp when the row was written.
    pub recorded_at: i64,
    /// Where the numbers came from.
    pub source: String,
    /// Window length in days.
    pub window_days: i64,
    /// Inclusive window start.
    pub window_start: i64,
    /// Window end.
    pub window_end: i64,
    /// Revision measured.
    pub source_rev: String,
    /// Deploys measured.
    pub deploy_count: i64,
    /// Failed deploys.
    pub deploys_failed: i64,
    /// Lead-time samples.
    pub lead_samples: i64,
    /// Median lead time in seconds.
    pub lead_p50_seconds: Option<i64>,
    /// 90th percentile lead time in seconds.
    pub lead_p90_seconds: Option<i64>,
    /// Median restore time in seconds.
    pub mttr_seconds: Option<i64>,
    /// Restored incidents.
    pub mttr_restored: i64,
    /// Unrestored incidents.
    pub mttr_unrestored: i64,
    /// Threshold breaches recorded.
    pub breach_count: i64,
    /// Pinned policy fingerprint.
    pub policy_fingerprint: String,
    /// Serialized derivation manifest.
    pub derivation: String,
}

/// Inserts a snapshot and returns its id.
///
/// # Errors
///
/// Returns an error when the insert statement fails.
pub async fn insert_dora_snapshot(
    conn: &Connection,
    snapshot: &NewDoraSnapshot<'_>,
) -> Result<i64> {
    let mut rows = conn
        .query(
            "INSERT INTO dora_snapshots (recorded_at, source, window_days, window_start, \
             window_end, source_rev, deploy_count, deploys_failed, lead_samples, \
             lead_p50_seconds, lead_p90_seconds, mttr_seconds, mttr_restored, \
             mttr_unrestored, breach_count, policy_fingerprint, derivation) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17) \
             RETURNING id",
            params!(
                unix_now(),
                snapshot.source,
                snapshot.window_days,
                snapshot.window_start,
                snapshot.window_end,
                snapshot.source_rev,
                snapshot.deploy_count,
                snapshot.deploys_failed,
                snapshot.lead_samples,
                snapshot.lead_p50_seconds,
                snapshot.lead_p90_seconds,
                snapshot.mttr_seconds,
                snapshot.mttr_restored,
                snapshot.mttr_unrestored,
                snapshot.breach_count,
                snapshot.policy_fingerprint,
                snapshot.derivation
            ),
        )
        .await?;
    let row = rows
        .next()
        .await?
        .ok_or_else(|| DbError::NotFound("dora snapshot id vanished after insert".to_string()))?;
    Ok(row.get(0)?)
}

/// Lists snapshots newest first (`limit = -1` returns every row).
///
/// # Errors
///
/// Returns an error when the query fails.
pub async fn list_dora_snapshots(conn: &Connection, limit: i64) -> Result<Vec<DoraSnapshotRow>> {
    let mut rows = conn
        .query(
            "SELECT id, recorded_at, source, window_days, window_start, window_end, source_rev, \
             deploy_count, deploys_failed, lead_samples, lead_p50_seconds, lead_p90_seconds, \
             mttr_seconds, mttr_restored, mttr_unrestored, breach_count, policy_fingerprint, \
             derivation FROM dora_snapshots ORDER BY id DESC LIMIT ?1",
            params!(limit),
        )
        .await?;
    let mut snapshots = Vec::new();
    while let Some(row) = rows.next().await? {
        snapshots.push(DoraSnapshotRow {
            id: row.get(0)?,
            recorded_at: row.get(1)?,
            source: row.get(2)?,
            window_days: row.get(3)?,
            window_start: row.get(4)?,
            window_end: row.get(5)?,
            source_rev: row.get(6)?,
            deploy_count: row.get(7)?,
            deploys_failed: row.get(8)?,
            lead_samples: row.get(9)?,
            lead_p50_seconds: row.get(10)?,
            lead_p90_seconds: row.get(11)?,
            mttr_seconds: row.get(12)?,
            mttr_restored: row.get(13)?,
            mttr_unrestored: row.get(14)?,
            breach_count: row.get(15)?,
            policy_fingerprint: row.get(16)?,
            derivation: row.get(17)?,
        });
    }
    Ok(snapshots)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    fn snapshot(derivation: &str) -> NewDoraSnapshot<'_> {
        NewDoraSnapshot {
            source: "git",
            window_days: 30,
            window_start: 1_000,
            window_end: 2_000,
            source_rev: "deadbeef",
            deploy_count: 2,
            deploys_failed: 1,
            lead_samples: 165,
            lead_p50_seconds: Some(865_179),
            lead_p90_seconds: Some(2_335_088),
            mttr_seconds: None,
            mttr_restored: 0,
            mttr_unrestored: 1,
            breach_count: 2,
            policy_fingerprint: "sha256:abc",
            derivation,
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn insert_roundtrips_every_column() {
        let dir = tempfile::tempdir().unwrap();
        let conn = crate::migrate::connect_and_migrate(dir.path())
            .await
            .unwrap();
        let derivation = r#"{"ranges":[{"tag":"v0.1.0"},{"tag":"v0.1.1"}]}"#;
        let id = insert_dora_snapshot(&conn, &snapshot(derivation))
            .await
            .unwrap();

        let rows = list_dora_snapshots(&conn, -1).await.unwrap();
        assert_eq!(rows.len(), 1);
        let row = &rows[0];
        assert_eq!(row.id, id);
        assert_eq!(row.source, "git");
        assert_eq!(row.source_rev, "deadbeef");
        assert_eq!(row.deploy_count, 2);
        assert_eq!(row.deploys_failed, 1);
        assert_eq!(row.lead_p50_seconds, Some(865_179));
        assert_eq!(row.lead_p90_seconds, Some(2_335_088));
        assert_eq!(row.mttr_seconds, None);
        assert_eq!(row.mttr_restored, 0);
        assert_eq!(row.mttr_unrestored, 1);
        assert_eq!(row.breach_count, 2);
        assert_eq!(row.policy_fingerprint, "sha256:abc");

        // The derivation manifest stays round-trippable, and its range count
        // must equal the deploy count it claims to explain.
        let parsed: serde_json::Value = serde_json::from_str(&row.derivation).unwrap();
        let range_count = i64::try_from(parsed["ranges"].as_array().unwrap().len()).unwrap();
        assert_eq!(range_count, row.deploy_count);
        assert!(row.recorded_at > 0);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn list_is_newest_first_and_respects_the_limit() {
        let dir = tempfile::tempdir().unwrap();
        let conn = crate::migrate::connect_and_migrate(dir.path())
            .await
            .unwrap();
        let first = insert_dora_snapshot(&conn, &snapshot("{}")).await.unwrap();
        let second = insert_dora_snapshot(&conn, &snapshot("{}")).await.unwrap();

        let rows = list_dora_snapshots(&conn, -1).await.unwrap();
        assert_eq!(
            rows.iter().map(|row| row.id).collect::<Vec<_>>(),
            vec![second, first]
        );
        assert_eq!(list_dora_snapshots(&conn, 1).await.unwrap().len(), 1);
    }
}
