//! Hash chain integrity verification for the workflow event log.

use std::path::Path;

use anyhow::Result;

/// Report on workflow event log hash chain integrity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChainReport {
    /// Hash chain is intact across all recorded events.
    Intact {
        /// Total number of verified events.
        count: i64,
    },
    /// First divergence / tampering detected at a specific sequence number.
    Tampered {
        /// Sequence number of the first tampered row.
        seq: i64,
    },
}

impl ChainReport {
    /// Creates an intact report with event count.
    #[must_use]
    pub fn intact(count: i64) -> Self {
        Self::Intact { count }
    }

    /// Creates a tampered report at sequence number `seq`.
    #[must_use]
    pub fn tampered(seq: i64) -> Self {
        Self::Tampered { seq }
    }

    /// Returns `true` if the chain is intact.
    #[must_use]
    pub fn is_intact(&self) -> bool {
        matches!(self, Self::Intact { .. })
    }
}

/// Recomputes the workflow event hash chain start-to-end and reports the first tampered row.
///
/// # Errors
///
/// Returns an error if opening or migrating the database fails.
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
    use do_harness_db::{NewTask, insert_task_with_event};

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
    async fn audit_chain_reports_intact_for_fresh_db() {
        let dir = tempfile::tempdir().unwrap();
        let report = audit_chain(dir.path()).await.unwrap();
        assert_eq!(report, ChainReport::intact(0));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn audit_chain_reports_intact_after_events() {
        let dir = tempfile::tempdir().unwrap();
        let conn = do_harness_db::connect_and_migrate(dir.path())
            .await
            .unwrap();
        insert_task_with_event(&conn, &new_task("t1"))
            .await
            .unwrap();
        insert_task_with_event(&conn, &new_task("t2"))
            .await
            .unwrap();
        drop(conn);

        let report = audit_chain(dir.path()).await.unwrap();
        assert_eq!(report, ChainReport::intact(2));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn audit_chain_detects_payload_tampering() {
        let dir = tempfile::tempdir().unwrap();
        let conn = do_harness_db::connect_and_migrate(dir.path())
            .await
            .unwrap();
        insert_task_with_event(&conn, &new_task("t1"))
            .await
            .unwrap();
        insert_task_with_event(&conn, &new_task("t2"))
            .await
            .unwrap();

        // Tamper with payload of seq 2
        conn.execute(
            "UPDATE workflow_events SET payload = '{\"kind\":\"TaskAdded\",\"id\":2,\"method\":\"mini\",\"title\":\"tampered\"}' WHERE seq = 2",
            (),
        )
        .await
        .unwrap();
        drop(conn);

        let report = audit_chain(dir.path()).await.unwrap();
        assert_eq!(report, ChainReport::tampered(2));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn audit_chain_detects_deleted_row() {
        let dir = tempfile::tempdir().unwrap();
        let conn = do_harness_db::connect_and_migrate(dir.path())
            .await
            .unwrap();
        insert_task_with_event(&conn, &new_task("t1"))
            .await
            .unwrap();
        insert_task_with_event(&conn, &new_task("t2"))
            .await
            .unwrap();

        // Delete row seq 1
        conn.execute("DELETE FROM workflow_events WHERE seq = 1", ())
            .await
            .unwrap();
        drop(conn);

        let report = audit_chain(dir.path()).await.unwrap();
        assert_eq!(report, ChainReport::tampered(2));
    }
}
