//! Seeds the `invariants` table from `plans/invariants.json`.
//!
//! Reads the machine-readable decision headers, validates them against the
//! [`DecisionHeader`] schema contract, and upserts them into libSQL.
//!
//! Usage: `seed_invariants [ROOT]` — `ROOT` defaults to the discovered
//! workspace root.

use std::path::PathBuf;

use anyhow::{Context, Result};
use do_harness_db::{connect_and_migrate, find_harness_root, seed_invariants};
use do_harness_types::DecisionHeader;

/// Path to the decision-header JSON file, relative to the workspace root.
const INVARIANTS_RELATIVE_PATH: &str = "plans/invariants.json";

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
    let json_path = root.join(INVARIANTS_RELATIVE_PATH);
    let json = std::fs::read_to_string(&json_path)
        .with_context(|| format!("failed to read {}", json_path.display()))?;
    let headers: Vec<DecisionHeader> = serde_json::from_str(&json)
        .context("invalid invariants.json: does not match DecisionHeader schema")?;

    let conn = connect_and_migrate(&root).await?;
    let written = seed_invariants(&conn, &headers).await?;

    println!("Seeded {written} invariants from {}", json_path.display());
    Ok(())
}
