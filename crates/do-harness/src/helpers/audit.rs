//! `do-harness helpers audit`: `RustSec` advisory scan via cargo-audit,
//! natively.
//!
//! This is the Rust implementation of `scripts/check-audit.sh`; the script is
//! now a thin wrapper that delegates here. The output contract and exit codes
//! are identical (fail-open locally, fail-closed on demand, retry with
//! advisory-db cache recovery), and the two are covered by the parity
//! fixtures in `tests/native_helpers.rs`.

use std::path::Path;

use crate::helpers::{HelperError, require_tools};

/// Advisory IDs ignored by policy (reasons live in `scripts/check-audit.sh`
/// and `deny.toml`; the flags die with the pins they guard).
const IGNORED_ADVISORIES: [&str; 3] = [
    "RUSTSEC-2026-0097",
    "RUSTSEC-2024-0449",
    "RUSTSEC-2026-0543",
];

/// Retry delay before the cache-recovery path, in seconds.
const AUDIT_RETRY_DELAY_SECS: u64 = 2;

/// Runs `cargo audit` with the policy ignores and `$@`.
fn run_audit(root: &Path, args: &[String]) -> (bool, String) {
    let mut command = std::process::Command::new("cargo");
    command
        .current_dir(root)
        .arg("audit")
        .arg("--deny")
        .arg("warnings");
    for advisory in IGNORED_ADVISORIES {
        command.arg("--ignore").arg(advisory);
    }
    command.args(args);
    crate::helpers::run_captured(command)
}

/// The shared advisory-db cache directory, if any.
fn advisory_db_path() -> Option<std::path::PathBuf> {
    let home = std::env::var("CARGO_HOME").ok().or_else(|| {
        std::env::var("HOME")
            .ok()
            .map(|home| format!("{home}/.cargo"))
    });
    home.map(|home| std::path::PathBuf::from(home).join("advisory-db"))
}

/// Whether the shared advisory-db cache exists.
fn advisory_db_exists() -> bool {
    advisory_db_path().is_some_and(|db| db.is_dir())
}
/// Best-effort clearing of the shared advisory-db cache before the final
/// retry; failures are ignored by design (locked or read-only files).
fn clear_advisory_db() {
    let home = std::env::var("CARGO_HOME").ok().or_else(|| {
        std::env::var("HOME")
            .ok()
            .map(|home| format!("{home}/.cargo"))
    });
    let Some(home) = home else {
        return;
    };
    let db = std::path::PathBuf::from(home).join("advisory-db");
    if !db.is_dir() {
        return;
    }
    println!("WARN: cargo audit failed twice; clearing advisory-db and retrying...");
    let _ = std::fs::remove_dir_all(&db);
}

/// Runs `do-harness helpers audit`.
pub fn run(root: &Path, args: &[String]) -> Result<(), HelperError> {
    let installed = std::process::Command::new("cargo")
        .args(["audit", "--version"])
        .status()
        .is_ok_and(|status| status.success());
    if !installed {
        if require_tools() {
            println!("FAIL: cargo-audit is required when CI=true or DO_HARNESS_REQUIRE_TOOLS=1.");
            return Err(HelperError::Violation(String::new()));
        }
        println!(
            "SKIP: cargo-audit not installed; skipping RustSec scan (cargo install cargo-audit --locked)."
        );
        return Ok(());
    }

    let (ok, first) = run_audit(root, args);
    if !first.is_empty() {
        println!("{first}");
    }
    if ok {
        return Ok(());
    }

    // Retry once before clearing anything: the advisory-db under CARGO_HOME
    // is shared with any other sandbox verify running at the same time, so a
    // lock or contention failure need not mean the cache is unusable.
    std::thread::sleep(std::time::Duration::from_secs(AUDIT_RETRY_DELAY_SECS));
    let (ok, _) = run_audit(root, args);
    if ok {
        return Ok(());
    }

    // Fall back to the original recovery for a corrupted or truncated
    // advisory-db: clear the cache and retry once more.
    if advisory_db_exists() {
        clear_advisory_db();
        let (ok, third) = run_audit(root, args);
        if !third.is_empty() {
            println!("{third}");
        }
        if ok {
            return Ok(());
        }
    }
    Err(HelperError::Violation(String::new()))
}
