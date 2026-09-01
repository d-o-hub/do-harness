//! Workflow event log hash chain audit.

use std::path::Path;

use anyhow::Result;

/// Report of workflow event log hash chain integrity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChainReport {
    /// Whether the event log chain is intact.
    pub intact: bool,
    /// Number of events verified.
    pub count: i64,
    /// Offending sequence number if tampering or divergence was detected.
    pub tampered_seq: Option<i64>,
}

impl ChainReport {
    /// Creates a report indicating an intact chain of `count` events.
    #[must_use]
    pub fn intact(count: i64) -> Self {
        Self {
            intact: true,
            count,
            tampered_seq: None,
        }
    }

    /// Creates a report indicating tampering was detected at `seq`.
    #[must_use]
    pub fn tampered(seq: i64) -> Self {
        Self {
            intact: false,
            count: 0,
            tampered_seq: Some(seq),
        }
    }
}

/// Recomputes the chain start-to-end; reports the first tampered row.
///
/// # Errors
///
/// Returns an error when database connection or migration fails.
pub async fn audit_chain(root: &Path) -> Result<ChainReport> {
    let conn = do_harness_db::connect_and_migrate(root).await?;
    let mut prev: Option<String> = None;
    let mut seq_expected = 1i64;
    for row in do_harness_db::list_events_ascending(&conn).await? {
        let expected = do_harness_db::chain_hash(prev.as_deref(), &row.canonical_payload);
        if row.seq != seq_expected || row.chain_hash.as_deref() != Some(expected.as_str()) {
            return Ok(ChainReport::tampered(row.seq));
        }
        prev = Some(expected);
        seq_expected += 1;
    }
    Ok(ChainReport::intact(seq_expected - 1))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use do_harness_db::{
        NewTask, advance_subtask_with_event, connect_and_migrate, insert_task_with_event,
    };

    fn new_task(title: &str) -> NewTask<'_> {
        NewTask {
            title,
            method: Some("mini"),
            subtask_index: 0,
            precondition: None,
            parent_id: None,
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn audit_chain_reports_intact_for_unmodified_events() {
        let dir = tempfile::tempdir().unwrap();
        let conn = connect_and_migrate(dir.path()).await.unwrap();
        let (id, _) = insert_task_with_event(&conn, &new_task("t1"))
            .await
            .unwrap();
        advance_subtask_with_event(&conn, id).await.unwrap();
        drop(conn);

        let report = audit_chain(dir.path()).await.unwrap();
        assert!(report.intact);
        assert_eq!(report.count, 2);
        assert_eq!(report.tampered_seq, None);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn audit_chain_detects_payload_tampering() {
        let dir = tempfile::tempdir().unwrap();
        let conn = connect_and_migrate(dir.path()).await.unwrap();
        let (id, _) = insert_task_with_event(&conn, &new_task("t1"))
            .await
            .unwrap();
        advance_subtask_with_event(&conn, id).await.unwrap();

        // Tamper with payload of row seq=2
        conn.execute(
            "UPDATE workflow_events SET payload = '{\"kind\":\"TaskAdvanced\",\"id\":1,\"subtask_index\":99}' WHERE seq = 2",
            (),
        )
        .await
        .unwrap();
        drop(conn);

        let report = audit_chain(dir.path()).await.unwrap();
        assert!(!report.intact);
        assert_eq!(report.tampered_seq, Some(2));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn audit_chain_detects_deleted_row() {
        let dir = tempfile::tempdir().unwrap();
        let conn = connect_and_migrate(dir.path()).await.unwrap();
        let (id, _) = insert_task_with_event(&conn, &new_task("t1"))
            .await
            .unwrap();
        advance_subtask_with_event(&conn, id).await.unwrap();

        // Delete row seq=1
        conn.execute("DELETE FROM workflow_events WHERE seq = 1", ())
            .await
            .unwrap();
        drop(conn);

        let report = audit_chain(dir.path()).await.unwrap();
        assert!(!report.intact);
        assert_eq!(report.tampered_seq, Some(2));
    }
}
