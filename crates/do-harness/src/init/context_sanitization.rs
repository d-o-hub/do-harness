//! Scaffolding for the optional context-sanitization check.
//!
//! The check is opt-in: `init` writes the helper script and an inert
//! configuration recipe, but neither the script nor the recipe runs unless
//! the adopter declares the sensor and enables it. An installed sanitizer
//! never enables it implicitly.
//!
//! Split out of `init.rs` so that file stays under the 450-line
//! decomposition threshold; the entry points are called by the parent module.

use std::path::Path;

use anyhow::Result;

use super::{InitOpts, InitReport, write_if_absent};

/// The canonical helper script, copied into the adopter's workspace.
const CHECK_CONTEXT_SANITIZATION: &str =
    include_str!("../../templates/scripts/check-context-sanitization.py");

/// The inert configuration recipe appended to a generated policy.
///
/// The recipe is commented out: the sensor is not part of any default pack
/// and no default signal set names it. An adopter that wants it uncomments
/// the block and adds the sensor name to its own `verification` and `release`
/// sets.
const CONFIG_HINT: &str = r#"
# Optional context-sanitization check.
#
# The harness does not mediate context: it never captures prompts, tool
# arguments, or vault contents. This sensor proves that a configured
# sanitizer transforms one synthetic fixture, and nothing more. The evidence
# is capability evidence about the fixture — it is not proof that a real
# coding client routes every outbound byte through the sanitizer. Client
# registration is a client-specific integration:
# https://github.com/d-oit/do-context-shield/blob/main/docs/client-integration.md
#
# The check is off unless DO_HARNESS_CONTEXT_SANITIZATION=enabled, and
# DO_CONTEXT_SHIELD_BIN must name the executable when it is on. An unset mode
# reports "SKIP: context-sanitization: not configured", which strict
# verification treats as missing verification rather than as a pass.
#
# [[sensors]]
# name = "context-sanitization"
# argv = ["python3", "scripts/check-context-sanitization.py"]
# severity = "error"
# timeout = 45
"#;

/// Writes the helper script and appends the inert configuration hint.
///
/// The script is written with the same `write_if_absent` convention as every
/// other scaffolded file, so an adopter's own version is preserved unless
/// `--force` is supplied.
pub(super) fn scaffold(root: &Path, opts: &InitOpts, report: &mut InitReport) -> Result<()> {
    write_if_absent(
        root,
        "scripts/check-context-sanitization.py",
        CHECK_CONTEXT_SANITIZATION,
        opts.force,
        report,
    )?;
    write_if_absent(
        root,
        "scripts/check-client-conformance.py",
        include_str!("../../templates/scripts/check-client-conformance.py"),
        opts.force,
        report,
    )?;
    Ok(())
}

/// Appends the commented opt-in recipe to a generated policy.
///
/// The recipe is inert: appending it changes no sensor's applicability and
/// adds no sensor to any signal set.
pub(super) fn append_config_hint(config: &mut String) {
    config.push_str(CONFIG_HINT);
}
