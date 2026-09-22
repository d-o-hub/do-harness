//! Web-language scaffolding: the audit runners and libraries `init` writes.
//!
//! Split out of `init.rs` so that file stays under the 450-line decomposition
//! threshold; the entry point is re-exported by the parent module.

use std::path::Path;

use anyhow::Result;

use super::{InitOpts, InitReport, write_if_absent};

/// Writes the web-pack sensor runners and the bundled web-ui audit library.
///
/// Everything lands under `scripts/` and is committed with the workspace so
/// local evidence and CI run the identical audit code (the sensor↔gate parity
/// rule from the adoption contract). Runners print "SKIP:" and exit 0 when
/// Playwright is missing — mirroring the sensor policy — so a workspace
/// without browsers still goes green on `init && verify`.
pub(super) fn scaffold_web_scripts(
    root: &Path,
    opts: &InitOpts,
    report: &mut InitReport,
) -> Result<()> {
    for (relative, body) in [
        (
            "scripts/viewport-audit.mjs",
            include_str!("../../templates/scripts/viewport-audit.mjs"),
        ),
        (
            "scripts/a11y-audit.mjs",
            include_str!("../../templates/scripts/a11y-audit.mjs"),
        ),
        (
            "scripts/console-audit.mjs",
            include_str!("../../templates/scripts/console-audit.mjs"),
        ),
        (
            "scripts/perf-audit.mjs",
            include_str!("../../templates/scripts/perf-audit.mjs"),
        ),
        (
            "scripts/visual-audit.mjs",
            include_str!("../../templates/scripts/visual-audit.mjs"),
        ),
        (
            "scripts/i18n-audit.mjs",
            include_str!("../../templates/scripts/i18n-audit.mjs"),
        ),
        (
            "scripts/web-ui/audit.test.mjs",
            include_str!("../../templates/scripts/web-ui/audit.test.mjs"),
        ),
        (
            "scripts/web-ui/audit.browser.test.mjs",
            include_str!("../../templates/scripts/web-ui/audit.browser.test.mjs"),
        ),
        (
            "scripts/web-ui/lib/audit.mjs",
            include_str!("../../templates/scripts/web-ui/lib/audit.mjs"),
        ),
        (
            "scripts/web-ui/lib/annotate.mjs",
            include_str!("../../templates/scripts/web-ui/lib/annotate.mjs"),
        ),
        (
            "scripts/web-ui/lib/page-probe.mjs",
            include_str!("../../templates/scripts/web-ui/lib/page-probe.mjs"),
        ),
        (
            "scripts/web-ui/lib/geometry.mjs",
            include_str!("../../templates/scripts/web-ui/lib/geometry.mjs"),
        ),
        (
            "scripts/web-ui/lib/contrast.mjs",
            include_str!("../../templates/scripts/web-ui/lib/contrast.mjs"),
        ),
        (
            "scripts/web-ui/lib/console-audit.mjs",
            include_str!("../../templates/scripts/web-ui/lib/console-audit.mjs"),
        ),
        (
            "scripts/web-ui/lib/a11y-audit.mjs",
            include_str!("../../templates/scripts/web-ui/lib/a11y-audit.mjs"),
        ),
        (
            "scripts/web-ui/lib/perf-audit.mjs",
            include_str!("../../templates/scripts/web-ui/lib/perf-audit.mjs"),
        ),
        (
            "scripts/web-ui/lib/visual-audit.mjs",
            include_str!("../../templates/scripts/web-ui/lib/visual-audit.mjs"),
        ),
        (
            "scripts/web-ui/lib/i18n-audit.mjs",
            include_str!("../../templates/scripts/web-ui/lib/i18n-audit.mjs"),
        ),
        (
            "scripts/web-ui/fixtures/clean.html",
            include_str!("../../templates/scripts/web-ui/fixtures/clean.html"),
        ),
        (
            "scripts/web-ui/fixtures/overlap.html",
            include_str!("../../templates/scripts/web-ui/fixtures/overlap.html"),
        ),
        (
            "scripts/web-ui/fixtures/overlap-descendant.html",
            include_str!("../../templates/scripts/web-ui/fixtures/overlap-descendant.html"),
        ),
        (
            "scripts/web-ui/fixtures/contrast.html",
            include_str!("../../templates/scripts/web-ui/fixtures/contrast.html"),
        ),
        (
            "scripts/web-ui/fixtures/app-shell.html",
            include_str!("../../templates/scripts/web-ui/fixtures/app-shell.html"),
        ),
    ] {
        write_if_absent(root, relative, body, opts.force, report)?;
    }
    Ok(())
}
