//! Fail-fast error-signature inspection for `do-harness errors`.

use std::path::Path;

use anyhow::{Context, Result};

use crate::report::Format;

/// Prints error signatures in the requested format.
///
/// # Errors
///
/// Returns an error when the state database cannot be opened.
pub async fn list(root: &Path, scope: Option<&str>, format: Format) -> Result<()> {
    let conn = do_harness_db::connect_and_migrate(root).await?;
    let signatures = do_harness_db::list_error_signatures(&conn, scope).await?;
    match format {
        Format::Text => {
            if signatures.is_empty() {
                println!("No error signatures recorded.");
            }
            for sig in &signatures {
                print!(
                    "{} attempt_count={} scope={}",
                    sig.signature, sig.attempt_count, sig.scope
                );
                println!();
                if let Some(msg) = &sig.message {
                    println!("  {msg}");
                }
            }
        }
        Format::Json => {
            println!(
                "{}",
                serde_json::to_string(&signatures).context("failed to serialize signatures")?
            );
        }
    }
    Ok(())
}

/// Clears error signatures, scoped by optional workstream and sensor.
///
/// Returns the number of rows removed.
///
/// # Errors
///
/// Returns an error when the state database cannot be opened.
pub async fn clear(root: &Path, scope: Option<&str>, sensor: Option<&str>) -> Result<usize> {
    let conn = do_harness_db::connect_and_migrate(root).await?;
    let removed = do_harness_db::clear_error_signatures(&conn, scope, sensor).await?;
    Ok(removed)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    #[tokio::test(flavor = "current_thread")]
    async fn list_prints_something_when_empty() {
        let dir = tempfile::tempdir().unwrap();
        list(dir.path(), None, Format::Text).await.unwrap();
    }

    #[tokio::test(flavor = "current_thread")]
    async fn clear_removes_scoped_rows() {
        let dir = tempfile::tempdir().unwrap();
        let conn = do_harness_db::connect_and_migrate(dir.path())
            .await
            .unwrap();
        do_harness_db::bump_error_signature(&conn, "sensor:clippy", "global", Some("m"))
            .await
            .unwrap();
        do_harness_db::bump_error_signature(&conn, "sensor:fmt", "global", Some("m"))
            .await
            .unwrap();
        drop(conn);

        let removed = clear(dir.path(), None, Some("sensor:clippy"))
            .await
            .unwrap();
        assert_eq!(removed, 1);
        let conn = do_harness_db::connect_and_migrate(dir.path())
            .await
            .unwrap();
        assert!(
            do_harness_db::get_error_signature(&conn, "sensor:clippy", "global")
                .await
                .unwrap()
                .is_none()
        );
        assert!(
            do_harness_db::get_error_signature(&conn, "sensor:fmt", "global")
                .await
                .unwrap()
                .is_some()
        );
    }
}
