//! Scaffolded file writers for `do-harness init`.
//!
//! Split from `init.rs` to keep the module under the repository line ceiling.

use std::fs;
use std::path::Path;

use anyhow::{Context, Result};

use super::{CHECK_RELEASE_PREFLIGHT, InitOpts, InitReport};

const CHECK_LOC: &str = include_str!("../../templates/scripts/check-loc.sh");
const CHECK_TRACKER_DRIFT: &str = include_str!("../../templates/scripts/check-tracker-drift.sh");
const CHECK_COMMITLINT: &str = include_str!("../../templates/scripts/check-commitlint.sh");
const CHECK_DEPS: &str = include_str!("../../templates/scripts/check-deps.sh");
const CHECK_AUDIT: &str = include_str!("../../templates/scripts/check-audit.sh");
const CHECK_COVERAGE: &str = include_str!("../../templates/scripts/check-coverage.sh");
const CHECK_TEST: &str = include_str!("../../templates/scripts/check-test.sh");
const NEXTEST_CONFIG: &str = include_str!("../../assets/nextest.toml");
const CRATE_MANIFEST: &str = include_str!("../../templates/crate/Cargo.toml.template");
const CRATE_LIB: &str = include_str!("../../templates/crate/src/lib.rs");

/// Writes the Rust-pack helper scripts and marks them executable.
pub(super) fn scaffold_scripts(
    root: &Path,
    opts: &InitOpts,
    report: &mut InitReport,
) -> Result<()> {
    for (relative, body) in [
        ("scripts/check-loc.sh", CHECK_LOC),
        ("scripts/check-deps.sh", CHECK_DEPS),
        ("scripts/check-audit.sh", CHECK_AUDIT),
        ("scripts/check-coverage.sh", CHECK_COVERAGE),
        ("scripts/check-test.sh", CHECK_TEST),
        (
            "scripts/check-release-preflight.sh",
            CHECK_RELEASE_PREFLIGHT,
        ),
    ] {
        if write_if_absent(root, relative, body, opts.force, report)? {
            crate::fs_perm::set_owner_exec(&root.join(relative))?;
        }
    }
    Ok(())
}

/// Writes the language-agnostic helper scripts every pack needs and marks them
/// executable.
///
/// The drift check reads a status document and `gh` (undeclared scripts are
/// inert until a config declares the `project-check` sensor). The commitlint
/// script is the managed `commit-msg` hook's dependency, so a pack without it
/// would block every commit after `init` + `hook install`.
pub(super) fn scaffold_shared_scripts(
    root: &Path,
    opts: &InitOpts,
    report: &mut InitReport,
) -> Result<()> {
    for (relative, body) in [
        ("scripts/check-tracker-drift.sh", CHECK_TRACKER_DRIFT),
        ("scripts/check-commitlint.sh", CHECK_COMMITLINT),
    ] {
        if write_if_absent(root, relative, body, opts.force, report)? {
            crate::fs_perm::set_owner_exec(&root.join(relative))?;
        }
    }
    Ok(())
}

/// Writes `body` to `root/relative`, skipping existing files unless `force`.
pub(super) fn write_if_absent(
    root: &Path,
    relative: &str,
    body: &str,
    force: bool,
    report: &mut InitReport,
) -> Result<bool> {
    let path = crate::managed_path::within(root, Path::new(relative))?;
    if path.exists() && !force {
        report.skipped.push(relative.to_owned());
        return Ok(false);
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    crate::managed_path::within(root, &path)?;
    crate::managed_path::write_atomic(&path, body.as_bytes())?;
    report.written.push(relative.to_owned());
    Ok(true)
}

/// Writes a minimal cargo crate when no `Cargo.toml` exists so a greenfield
/// workspace passes the rust sensor pack immediately (`init && verify` is
/// green on an empty tree).
///
/// Existing crates are never touched — not even with `--force` — because
/// overwriting a real manifest would be destructive; the crate files are
/// greenfield-only scaffolding.
pub(super) fn scaffold_crate(root: &Path, report: &mut InitReport) -> Result<()> {
    if root.join("Cargo.toml").exists() {
        report.skipped.push("Cargo.toml".to_owned());
        return Ok(());
    }
    write_if_absent(root, "Cargo.toml", CRATE_MANIFEST, false, report)?;
    write_if_absent(root, "src/lib.rs", CRATE_LIB, false, report)?;
    write_if_absent(root, ".config/nextest.toml", NEXTEST_CONFIG, false, report)?;
    Ok(())
}
