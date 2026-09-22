//! Database-facing commands: `init-db`, `maintenance`, and `seed`.
//!
//! Split out of `commands.rs` so that file stays under the 450-line
//! decomposition threshold; the `commands` re-exports keep the call sites in
//! `main.rs` unchanged.

use std::path::Path;

use anyhow::{Context as _, Result};

/// Options for `init-db`.
#[derive(Debug, Clone, Copy)]
pub struct InitDbOpts {
    /// Report pending migrations and exit non-zero when any are pending.
    pub check: bool,
    /// Report migration state without applying anything (exit 0).
    pub dry_run: bool,
    /// Skip the interactive confirmation prompt.
    pub yes: bool,
}

/// Applies pending migrations and reports the number applied.
///
/// Interactive terminals confirm before applying pending migrations; hooks,
/// CI, and piped invocations apply without prompting (pass `--yes` to force
/// non-interactive behavior explicitly). `--check`/`--dry-run` never write.
///
/// # Errors
///
/// Returns an error when the health probe fails, `--check` sees an unmigrated
/// database, or applying migrations fails.
pub async fn init_db(root: &Path, opts: &InitDbOpts) -> Result<()> {
    use std::io::IsTerminal as _;

    let health = crate::dbcheck::probe(root).await?;
    let pending = matches!(health, crate::dbcheck::DbHealth::Pending { .. });
    if opts.check || opts.dry_run {
        let (mark, line) = health.render();
        println!("[{mark}] {line}");
        if opts.check && !matches!(health, crate::dbcheck::DbHealth::Current) {
            anyhow::bail!("database is not at the current migration version");
        }
        return Ok(());
    }
    if pending && !opts.yes && std::io::stdin().is_terminal() && !confirm_migrations(&health)? {
        anyhow::bail!("migration cancelled; re-run with --yes to apply");
    }
    let conn = do_harness_db::connect_and_migrate(root).await?;
    let count = do_harness_db::count_migrations(&conn).await?;
    println!("Done. Applied schema migrations: {count}");
    Ok(())
}

/// Prompts on the terminal for confirmation to apply pending migrations.
fn confirm_migrations(health: &crate::dbcheck::DbHealth) -> Result<bool> {
    use std::io::{BufRead as _, Write as _};

    let (_, line) = health.render();
    print!("{line}\nApply pending migrations now? [y/N] ");
    std::io::stdout()
        .flush()
        .context("failed to flush prompt")?;
    let mut answer = String::new();
    std::io::stdin()
        .lock()
        .read_line(&mut answer)
        .context("failed to read confirmation")?;
    Ok(matches!(
        answer.trim().to_ascii_lowercase().as_str(),
        "y" | "yes"
    ))
}

/// Prunes old beats and compacts the state database.
///
/// `--prune-beats <days>` deletes beats older than the cutoff while keeping at
/// least `keep_per_task` most-recent beats per task; the database is then
/// `VACUUM`-compacted. Without the flag only `VACUUM` runs.
///
/// # Errors
///
/// Returns an error when the database cannot be opened or pruned.
pub async fn maintenance(root: &Path, prune_beats: Option<i64>, keep_per_task: i64) -> Result<()> {
    let conn = do_harness_db::connect_and_migrate(root).await?;
    if let Some(days) = prune_beats {
        let cutoff = do_harness_db::unix_now().saturating_sub(days.max(0).saturating_mul(86_400));
        let deleted = do_harness_db::prune_beats(&conn, cutoff, keep_per_task.max(0)).await?;
        println!(
            "Pruned {deleted} beat(s) older than {days} day(s) (kept >= {keep_per_task} per task)"
        );
    }
    do_harness_db::vacuum(&conn).await?;
    println!("VACUUM complete");
    Ok(())
}

/// Seeds the `invariants` table from `plans/invariants.json`.
pub async fn seed(root: &Path, prune: bool) -> Result<()> {
    let written = crate::init::seed_invariants(root, prune).await?;
    println!(
        "Seeded {written} invariants from {}",
        root.join("plans/invariants.json").display()
    );
    Ok(())
}
