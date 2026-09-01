//! `doctor` subcommand: binary resolution, git hook health, and state-database
//! migration skew.
//!
//! Advisory findings (absent hooks, uninitialized database) print warnings;
//! required-check failures (binary missing, database newer than this binary)
//! make doctor exit non-zero with every reason carried in the error so CI and
//! other non-interactive callers can classify the failure.

use std::path::Path;

use anyhow::Result;

use crate::dbcheck;
use crate::hook_script::BinSource;
use crate::hooks;

/// Options controlling doctor diagnostic behavior.
#[derive(Debug, Clone, Copy, Default)]
pub struct DoctorOpts {
    /// Elevate advisory warnings to fatal failure checks.
    pub strict: bool,
}

/// Runs diagnostic checks covering binary resolution, git hook status, and
/// state-database migration skew.
///
/// # Errors
///
/// Returns an error listing every failed required check, or when git repo
/// discovery fails.
pub async fn run(root: &Path, opts: &DoctorOpts) -> Result<()> {
    let git_dir = hooks::find_git_dir(root)?;
    let status = hooks::status(&git_dir, root);

    let mut failures: Vec<String> = Vec::new();

    println!("Doctor Summary:");
    println!("===============");

    // 0. Workspace Config Check
    match crate::config::load(root, None) {
        Ok(_) => println!("  [OK] config: do-harness.toml valid"),
        Err(err) => {
            println!("  [FAIL] config: invalid ({err:#})");
            failures.push(format!("config invalid: {err:#}"));
        }
    }

    // 1. Binary Resolution Check
    let bin_path = status.binary.path();
    let bin_source_desc = describe_binary(&status.binary);
    if status.binary.present() {
        println!(
            "  [OK] Binary resolution: {bin_source_desc} ({})",
            bin_path.display()
        );
    } else {
        println!(
            "  [FAIL] Binary resolution: {bin_source_desc} ({}) - file missing",
            bin_path.display()
        );
        failures.push(format!("binary missing at {}", bin_path.display()));
    }

    // Target directory warning (failure under --strict)
    if status.binary.is_in_target_dir() {
        eprintln!(
            "warning: resolved do-harness binary is under Cargo target/; cargo clean will remove it: {}",
            bin_path.display()
        );
        if opts.strict {
            failures.push(format!(
                "resolved do-harness binary is under Cargo target/: {}",
                bin_path.display()
            ));
        }
    }

    // 2. Git Hooks Check (absence is advisory; install via `hook install`)
    for (name, installed) in [
        ("pre-commit", status.pre_commit),
        ("pre-push", status.pre_push),
        ("commit-msg", status.commit_msg),
    ] {
        println!(
            "  [{}] {name} hook: {}",
            if installed { "OK" } else { "WARN" },
            if installed { "installed" } else { "absent" }
        );
        if !installed && opts.strict {
            failures.push(format!("{name} hook absent"));
        }
    }

    // 3. State Database Skew Check
    match dbcheck::probe(root).await {
        Ok(health) => {
            let (mark, line) = health.render();
            println!("  [{mark}] state database: {line}");
            if health.is_blocking() || (opts.strict && mark == "WARN") {
                failures.push(format!("state database: {line}"));
            }
        }
        Err(err) => {
            println!("  [FAIL] state database: unreadable ({err:#})");
            failures.push(format!("state database unreadable: {err:#}"));
        }
    }

    // 4. Workflow Event Hash Chain Integrity Check (informational)
    if do_harness_db::db_path(root).exists() {
        match crate::audit::audit_chain(root).await {
            Ok(report) => match report {
                crate::audit::ChainReport::Intact { count } => {
                    println!("  [OK] event chain: intact ({count} event(s))");
                }
                crate::audit::ChainReport::Tampered { seq } => {
                    println!("  [WARN] event chain: tampered at seq {seq}");
                    if opts.strict {
                        failures.push(format!("event chain tampered at seq {seq}"));
                    }
                }
            },
            Err(err) => {
                println!("  [WARN] event chain: unreadable ({err:#})");
                if opts.strict {
                    failures.push(format!("event chain unreadable: {err:#}"));
                }
            }
        }
    }

    if failures.is_empty() {
        println!("\nDoctor checks passed.");
        Ok(())
    } else {
        anyhow::bail!("doctor check failed: {}", failures.join("; "))
    }
}

/// Describes where the `do-harness` binary resolves from.
pub(crate) fn describe_binary(source: &BinSource) -> String {
    match source {
        BinSource::Env(path) => format!("env:{}", path.display()),
        BinSource::Path(path) => format!("path:{}", path.display()),
        BinSource::Repo(path) => format!("repo:{}", path.display()),
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use std::fs;
    use std::path::PathBuf;

    fn fake_repo_with_git() -> (tempfile::TempDir, PathBuf) {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().to_path_buf();
        let git_dir = root.join(".git");
        fs::create_dir_all(git_dir.join("hooks")).unwrap();
        (temp, root)
    }

    /// Stubs a target/release/do-harness so the binary-resolution check passes.
    fn stub_binary(root: &Path) {
        let bin_path = root.join("target/release/do-harness");
        fs::create_dir_all(bin_path.parent().unwrap()).unwrap();
        fs::write(&bin_path, "stub").unwrap();
    }

    #[tokio::test(flavor = "current_thread")]
    async fn fails_when_binary_missing_and_names_the_reason() {
        let (_temp, root) = fake_repo_with_git();

        let result = run(&root, &DoctorOpts::default()).await;

        let message = result.unwrap_err().to_string();
        assert!(message.contains("doctor check failed"));
        assert!(message.contains("binary missing"));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn succeeds_when_binary_present_and_database_absent() {
        let (_temp, root) = fake_repo_with_git();
        stub_binary(&root);

        assert!(run(&root, &DoctorOpts::default()).await.is_ok());
    }

    /// A database written by a newer harness is a required-check failure: the
    /// binary cannot persist anything until it is rebuilt, and the error must
    /// say so explicitly.
    #[tokio::test(flavor = "current_thread")]
    async fn fails_when_state_database_is_future() {
        let (_temp, root) = fake_repo_with_git();
        stub_binary(&root);
        let conn = do_harness_db::connect_and_migrate(&root).await.unwrap();
        conn.execute(
            "INSERT INTO schema_migrations (version, name, applied_at) VALUES (9999, 'future', 0)",
            (),
        )
        .await
        .unwrap();
        drop(conn);

        let result = run(&root, &DoctorOpts::default()).await;

        let message = result.unwrap_err().to_string();
        assert!(message.contains("doctor check failed"));
        assert!(message.contains("rebuild"));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn fails_in_strict_mode_when_warnings_present() {
        let (_temp, root) = fake_repo_with_git();
        stub_binary(&root);

        let result = run(&root, &DoctorOpts { strict: true }).await;

        let message = result.unwrap_err().to_string();
        assert!(message.contains("doctor check failed"));
        assert!(message.contains("hook absent"));
    }
}
