//! Initializes the do-harness agent-state database and schema.
//!
//! Usage: `init_db [ROOT]` — `ROOT` defaults to the discovered workspace root.

use std::path::PathBuf;

use anyhow::{Context, Result};
use do_harness_db::{connect_and_migrate, db_path, find_harness_root};

/// Resolves the workspace root from the optional positional argument or by
/// walking up from the current directory.
///
/// # Errors
///
/// Returns an error when the current directory cannot be read or no harness
/// root marker is found.
fn resolve_root(arg: Option<&str>) -> Result<PathBuf> {
    if let Some(root) = arg {
        return Ok(PathBuf::from(root));
    }
    let cwd = std::env::current_dir().context("failed to read current directory")?;
    find_harness_root(&cwd)
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    let root = resolve_root(std::env::args().nth(1).as_deref())?;
    let path = db_path(&root);
    println!("Initializing agent-state database at: {}", path.display());

    let conn = connect_and_migrate(&root).await?;

    let mut rows = conn
        .query(
            "SELECT COUNT(*) FROM schema_migrations",
            libsql::params::Params::None,
        )
        .await?;
    let version_count = match rows.next().await? {
        Some(row) => row.get::<i64>(0)?,
        None => anyhow::bail!("schema_migrations is unexpectedly empty"),
    };

    println!("Done. Applied schema migrations: {version_count}");
    Ok(())
}
