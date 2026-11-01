//! Pre-migration snapshot of the agent-state database (FU-01).

use std::path::Path;

use libsql::Connection;

use crate::error::{DbError, Result};

/// Snapshots the state database beside itself as `<file>.db.bak`.
///
/// The connection runs in WAL mode (see [`crate::migrate::connect`]), so
/// committed pages can still live in the `-wal` sidecar. Copying the main file
/// then yields either an image that silently predates the last commits or a
/// torn one — reproduced as `database disk image is malformed`. A
/// `wal_checkpoint(TRUNCATE)` cannot repair that: a concurrent reader pins the
/// WAL and its failure is easy to miss.
///
/// `VACUUM INTO` copies through `SQLite` itself, producing a transactionally
/// consistent snapshot that includes WAL content without disturbing the live
/// database. The snapshot is staged beside the backup and renamed into place
/// only once it exists, so a failed snapshot leaves the previous backup intact:
/// that file is the recovery path for the destructive migrations (`0006`
/// deletes rows, `0011` rebuilds tables).
pub(super) async fn backup_state_file(conn: &Connection, path: &Path) -> Result<()> {
    let backup = path.with_extension("db.bak");
    let staging = path.with_extension("db.bak.tmp");
    // `VACUUM INTO` refuses an existing destination, so clear a staging file
    // from an interrupted run. The published backup stays untouched until the
    // rename below.
    match std::fs::remove_file(&staging) {
        Ok(()) => {}
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
        Err(source) => {
            return Err(DbError::Backup {
                path: staging,
                source,
            });
        }
    }
    let staging_arg = staging
        .to_str()
        .ok_or_else(|| DbError::Backup {
            path: staging.clone(),
            source: std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "state database path is not valid UTF-8",
            ),
        })?
        .to_owned();
    conn.execute("VACUUM INTO ?1", libsql::params!(staging_arg))
        .await?;
    std::fs::rename(&staging, &backup).map_err(|source| DbError::Backup {
        path: backup,
        source,
    })
}
