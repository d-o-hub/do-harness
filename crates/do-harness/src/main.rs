//! Unified CLI for the do-harness agent execution harness.
//!
//! Entrypoints: `verify` (computational sensors), `list` (sensor names),
//! `init-db` (migrations), and `seed` (architecture invariants from
//! `plans/invariants.json`).

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{Context, Result};
use clap::{ArgAction, Parser, Subcommand};

use crate::report::Format;

mod config;
mod hooks;
mod report;
mod sensors;

/// Unified entrypoint for harness sensors and database maintenance.
#[derive(Debug, Parser)]
#[command(name = "do-harness", about = "do-harness agent execution harness CLI")]
struct Cli {
    /// Workspace root override (default: walk up from cwd).
    #[arg(long, global = true)]
    root: Option<PathBuf>,
    /// Explicit path to do-harness.toml.
    #[arg(long, global = true)]
    config: Option<PathBuf>,
    #[command(subcommand)]
    command: Command,
}

/// Available subcommands.
#[derive(Debug, Subcommand)]
enum Command {
    /// Run computational sensors.
    Verify {
        /// Halt at the first failing sensor.
        #[arg(long)]
        fail_fast: bool,
        /// Output format.
        #[arg(long, value_enum, default_value_t = Format::Text)]
        format: Format,
        /// Run only the named sensor (repeatable).
        #[arg(long = "only", action = ArgAction::Append)]
        only: Vec<String>,
    },
    /// List sensor names.
    List {
        /// Output format.
        #[arg(long, value_enum, default_value_t = Format::Text)]
        format: Format,
    },
    /// Apply pending database migrations.
    InitDb,
    /// Seed invariants from plans/invariants.json.
    Seed,
    /// Manage git hooks that run `do-harness verify`.
    Hook {
        #[command(subcommand)]
        action: HookAction,
    },
}

/// Available hook-management actions.
#[derive(Debug, Copy, Clone, Subcommand)]
enum HookAction {
    /// Write pre-commit and pre-push hooks into `.git/hooks/`.
    Install {
        /// Overwrite foreign (unmanaged) hook files.
        #[arg(long)]
        force: bool,
    },
    /// Remove managed hooks, leaving foreign hook files untouched.
    Uninstall,
    /// Show whether the managed hooks and release binary are present.
    Status,
}

/// Classified CLI failure carrying its process exit code.
enum CliError {
    /// Usage, config, or discovery problems: exit 2.
    Usage(anyhow::Error),
    /// Sensor verification failed: exit 1.
    Verify(anyhow::Error),
}

impl CliError {
    /// The process exit code for this error class.
    fn exit_code(&self) -> u8 {
        match self {
            CliError::Usage(_) => 2,
            CliError::Verify(_) => 1,
        }
    }
}

impl std::fmt::Display for CliError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CliError::Usage(err) | CliError::Verify(err) => write!(f, "{err:#}"),
        }
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("error: {err:#}");
            ExitCode::from(err.exit_code())
        }
    }
}

/// Dispatches the parsed CLI and classifies failures.
async fn run(cli: Cli) -> std::result::Result<(), CliError> {
    let root = resolve_root(cli.root.as_deref()).map_err(CliError::Usage)?;
    match cli.command {
        Command::Verify {
            fail_fast,
            format,
            only,
        } => {
            let cfg = config::load(&root, cli.config.as_deref()).map_err(CliError::Usage)?;
            let opts = sensors::VerifyOpts { fail_fast, only };
            match sensors::verify(&cfg, &root, &opts) {
                Ok(report) => {
                    report::print_report(&report, format);
                    if report.ok {
                        Ok(())
                    } else {
                        Err(CliError::Verify(anyhow::anyhow!(
                            "{} sensor(s) failed",
                            report.failed.len()
                        )))
                    }
                }
                Err(err) => Err(CliError::Usage(err)),
            }
        }
        Command::List { format } => {
            let cfg = config::load(&root, cli.config.as_deref()).map_err(CliError::Usage)?;
            report::print_names(&cfg.sensor_names(), format);
            Ok(())
        }
        Command::InitDb => init_db(&root).await.map_err(CliError::Usage),
        Command::Seed => seed(&root).await.map_err(CliError::Usage),
        Command::Hook { action } => {
            hook(&root, cli.config.as_deref(), action).map_err(CliError::Usage)
        }
    }
}

/// Dispatches hook management using the configured sensor split.
///
/// # Errors
///
/// Returns an error when no git repository is found or a hook file cannot be
/// written or removed.
fn hook(root: &Path, config_path: Option<&Path>, action: HookAction) -> Result<()> {
    let cwd = std::env::current_dir().context("failed to read current directory")?;
    let git_dir = hooks::find_git_dir(&cwd)?;
    match action {
        HookAction::Install { force } => {
            let cfg = config::load(root, config_path)?;
            hooks::install(&git_dir, &cfg.hooks.pre_commit, &cfg.hooks.pre_push, force)?;
            println!(
                "Installed pre-commit and pre-push hooks in {}",
                git_dir.display()
            );
        }
        HookAction::Uninstall => {
            hooks::uninstall(&git_dir)?;
            println!("Removed managed hooks from {}", git_dir.display());
        }
        HookAction::Status => {
            let status = hooks::status(&git_dir, root);
            println!(
                "pre-commit: {}  pre-push: {}  release binary: {}",
                if status.pre_commit {
                    "installed"
                } else {
                    "absent"
                },
                if status.pre_push {
                    "installed"
                } else {
                    "absent"
                },
                if status.binary_exists {
                    "present"
                } else {
                    "missing"
                }
            );
        }
    }
    Ok(())
}

/// Applies pending migrations and reports the number applied.
async fn init_db(root: &Path) -> Result<()> {
    let conn = do_harness_db::connect_and_migrate(root).await?;
    let mut rows = conn
        .query("SELECT COUNT(*) FROM schema_migrations", ())
        .await?;
    let count = match rows.next().await? {
        Some(row) => row.get::<i64>(0)?,
        None => anyhow::bail!("schema_migrations is unexpectedly empty"),
    };
    println!("Done. Applied schema migrations: {count}");
    Ok(())
}

/// Seeds the `invariants` table from `plans/invariants.json`.
async fn seed(root: &Path) -> Result<()> {
    let json_path = root.join("plans/invariants.json");
    let json = std::fs::read_to_string(&json_path)
        .with_context(|| format!("failed to read {}", json_path.display()))?;
    let headers: Vec<do_harness_types::DecisionHeader> = serde_json::from_str(&json)
        .context("invalid invariants.json: does not match DecisionHeader schema")?;

    let conn = do_harness_db::connect_and_migrate(root).await?;
    let written = do_harness_db::seed_invariants(&conn, &headers).await?;

    println!("Seeded {written} invariants from {}", json_path.display());
    Ok(())
}

/// Resolves the workspace root: explicit override or walk up from cwd.
///
/// # Errors
///
/// Returns an error when the explicit root is not a directory or no harness
/// root can be discovered from the current directory.
fn resolve_root(explicit: Option<&Path>) -> Result<PathBuf> {
    if let Some(path) = explicit {
        if !path.is_dir() {
            anyhow::bail!("root is not a directory: {}", path.display());
        }
        Ok(path.to_path_buf())
    } else {
        let cwd = std::env::current_dir().context("failed to read current directory")?;
        do_harness_db::find_harness_root(&cwd)
    }
}
