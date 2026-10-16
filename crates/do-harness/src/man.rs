//! `do-harness man`: renders the CLI's man page.
//!
//! `clap_mangen` indexes subcommands as `do-harness-<name>(1)`, which describes a
//! page per subcommand. This repository ships a single page, so the index names
//! the *invocations* instead of pointing at pages that are never generated —
//! `man_sync` fails when the committed page and this output disagree.

use std::path::Path;

use clap::CommandFactory;

use crate::CliError;
use crate::cli::Cli;

/// Renders `do-harness.1` into `dir`.
///
/// # Errors
///
/// Returns a usage error when `dir` cannot be created or written, or when the
/// rendered page is not valid UTF-8.
pub(crate) fn render_page(dir: &Path) -> Result<(), CliError> {
    let man = clap_mangen::Man::new(Cli::command());
    let mut buffer = Vec::new();
    man.render(&mut buffer)
        .map_err(|e| CliError::Usage(e.into()))?;
    let page = String::from_utf8(buffer).map_err(|e| CliError::Usage(e.into()))?;
    std::fs::create_dir_all(dir).map_err(|e| CliError::Usage(e.into()))?;
    std::fs::write(dir.join("do-harness.1"), rewrite_subcommand_refs(&page))
        .map_err(|e| CliError::Usage(e.into()))
}

/// Rewrites `do\-harness\-<name>(1)` index lines to `do\-harness <name>`.
///
/// Only whole lines are rewritten: the descriptions that follow them are
/// untouched, and prose that happens to mention a page keeps its text.
fn rewrite_subcommand_refs(page: &str) -> String {
    const PREFIX: &str = r"do\-harness\-";
    let mut out = String::with_capacity(page.len());
    for line in page.split_inclusive('\n') {
        let trimmed = line.trim_end_matches('\n');
        match trimmed
            .strip_prefix(PREFIX)
            .and_then(|rest| rest.strip_suffix("(1)"))
        {
            Some(name) => {
                out.push_str("do\\-harness ");
                out.push_str(&name.replace("\\-", "-"));
                out.push('\n');
            }
            None => out.push_str(line),
        }
    }
    out
}
