//! `do-harness helpers deps`: dependency-direction and cargo-deny gate,
//! natively.
//!
//! This is the Rust implementation of `scripts/check-deps.sh`; the script is
//! now a thin wrapper that delegates here. The output contract and exit codes
//! are identical (including the fail-open-locally / fail-closed-on-demand
//! policy for missing tools), and the two are covered by the parity fixtures
//! in `tests/native_helpers.rs`.
//!
//! The repo-local `scripts/check-package-contract.sh` bolt-on is invoked
//! through the standard bash resolution when present: it is an extra,
//! repository-specific contract (npm manifests, `cargo package --list`
//! enumeration) that is not part of the helper contract the issue names.

use std::path::Path;

use crate::helpers::{HelperError, require_tools, run_cargo};

/// Storage/adapter crates the types crate must never depend on.
const FORBIDDEN_TYPES_DEPENDENCIES: [&str; 4] = [
    "do-harness-db",
    "do-harness-core",
    "do-harness-adapters",
    "do-harness-cli",
];

/// First-token names banned from the types crate's normal-dependency closure.
const FORBIDDEN_TYPES_CLOSURE: [&str; 5] = [
    "do-harness-db",
    "do-harness",
    "guardian-proxy",
    "libsql",
    "do-harness-libsql",
];

/// First-token names banned from guardian-proxy's normal-dependency closure.
const FORBIDDEN_PROXY_CLOSURE: [&str; 1] = ["do-harness"];

/// Reads `relative` under `root` as text; missing files yield `None`.
fn read_manifest(root: &Path, relative: &str) -> Option<String> {
    std::fs::read_to_string(root.join(relative)).ok()
}

/// Whether any of `needles` appears as a substring of `text` (the script's
/// `grep -qE` equivalent for plain literal alternations).
fn contains_any(text: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| text.contains(needle))
}

/// Whether the first whitespace-delimited token of `line` is one of `names`
/// (the script's `^(name|...)( |$)` equivalent).
fn first_token_in(line: &str, names: &[&str]) -> bool {
    let token = line.split_whitespace().next().unwrap_or("");
    names.contains(&token)
}

/// Runs `cargo tree` closure checks: a tree failure is a hard error under
/// required-tools mode and a SKIP otherwise, mirroring the script.
fn check_closure(
    root: &Path,
    package: &str,
    forbidden: &[&str],
    fail_message: &str,
    skip_message: &str,
    fail_message_required: &str,
) -> Result<bool, HelperError> {
    let (ok, output) = run_cargo(
        root,
        &[
            "tree", "-p", package, "--edges", "normal", "--prefix", "none",
        ],
    );
    if ok {
        if output.lines().any(|line| first_token_in(line, forbidden)) {
            println!("FAIL: {fail_message}");
            return Ok(false);
        }
        return Ok(true);
    }
    if require_tools() {
        println!("FAIL: {fail_message_required}");
        return Ok(false);
    }
    println!("SKIP: {skip_message}");
    Ok(true)
}

/// Runs the package-contract bolt-on when the repository ships it.
fn check_package_contract(root: &Path, failed: &mut bool) {
    let path = root.join("scripts/check-package-contract.sh");
    if !path.is_file() {
        return;
    }
    let mut bash = crate::shell::bash();
    bash.current_dir(root).arg(&path);
    let Ok(output) = bash.output() else {
        if require_tools() {
            println!("FAIL: check-package-contract.sh could not be run under required-tools mode.");
            *failed = true;
        } else {
            println!("SKIP: bash unavailable; skipping package contract check (install Git Bash).");
        }
        return;
    };
    if !output.status.success() {
        println!("{}", String::from_utf8_lossy(&output.stdout));
        println!("{}", String::from_utf8_lossy(&output.stderr));
        *failed = true;
    }
}

/// Runs `do-harness helpers deps`.
pub fn run(root: &Path) -> Result<(), HelperError> {
    let mut failed = false;

    // Rule 1: do-harness-types (schema) must NOT depend on storage/adapters.
    if let Some(manifest) = read_manifest(root, "crates/types/Cargo.toml") {
        if contains_any(&manifest, &FORBIDDEN_TYPES_DEPENDENCIES) {
            println!("FAIL: do-harness-types must not depend on storage or adapters.");
            failed = true;
        }
    }

    // Rule 2a: real closure check over `cargo tree`, not just the manifest.
    match check_closure(
        root,
        "do-harness-types",
        &FORBIDDEN_TYPES_CLOSURE,
        "do-harness-types closure must not contain storage or adapters.",
        "cargo tree unavailable; skipping transitive closure check (ensure cargo is installed).",
        "cargo tree failed under required-tools mode.",
    )? {
        true => {}
        false => failed = true,
    }

    // Rule 2b: guardian-proxy (adjacent adapter) must NOT depend on the CLI.
    match check_closure(
        root,
        "guardian-proxy",
        &FORBIDDEN_PROXY_CLOSURE,
        "guardian-proxy must not depend on the do-harness CLI.",
        "cargo tree unavailable; skipping guardian-proxy closure check (ensure cargo is installed).",
        "cargo tree failed under required-tools mode.",
    )? {
        true => {}
        false => failed = true,
    }

    // cargo-deny: fails open locally, fails closed on demand.
    let deny = std::process::Command::new("cargo")
        .args(["deny", "--version"])
        .status()
        .is_ok_and(|status| status.success());
    if deny {
        let (ok, output) = run_cargo(root, &["deny", "check"]);
        if !ok {
            print!("{output}");
            failed = true;
        }
    } else if require_tools() {
        println!("FAIL: cargo-deny is required when CI=true or DO_HARNESS_REQUIRE_TOOLS=1.");
        failed = true;
    } else {
        println!(
            "SKIP: cargo-deny not installed; skipping deny check (cargo install cargo-deny --locked)."
        );
    }

    check_package_contract(root, &mut failed);

    if failed {
        return Err(HelperError::Violation(String::new()));
    }
    println!("check-deps OK.");
    Ok(())
}
