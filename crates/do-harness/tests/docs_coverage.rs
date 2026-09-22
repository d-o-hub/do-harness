//! Docs coverage: every CLI subcommand is documented, and every documented
//! command exists.
//!
//! `docs/cli.md` is the CLI reference and `README.md` the command table. Both
//! drifted silently whenever a command was added (`loc`, `split`, `overlap`,
//! `version`, `init-db`, and `seed` were reachable but undocumented), so the
//! coverage is asserted here instead of reviewed by hand.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Repository root, resolved from this test binary's manifest directory.
fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The subcommands `--help` advertises; `help` itself is not a feature.
fn subcommands() -> BTreeSet<String> {
    let output = Command::new(env!("CARGO_BIN_EXE_do-harness"))
        .arg("--help")
        .output()
        .expect("run do-harness --help");
    assert!(output.status.success(), "--help must exit 0");
    let text = String::from_utf8_lossy(&output.stdout).into_owned();

    let mut names = BTreeSet::new();
    let mut in_commands = false;
    for line in text.lines() {
        if line.starts_with("Commands:") {
            in_commands = true;
            continue;
        }
        if !in_commands {
            continue;
        }
        if line.trim().is_empty() {
            break;
        }
        if let Some(name) = line.split_whitespace().next() {
            if name != "help" {
                names.insert(name.to_owned());
            }
        }
    }
    assert!(
        !names.is_empty(),
        "no subcommands parsed from --help:\n{text}"
    );
    names
}

/// Commands named by level-3 headings in the reference, e.g. a heading for
/// `verify` or `pr no-effect`.
fn documented_commands(docs: &str) -> BTreeSet<String> {
    docs.lines()
        .filter_map(|line| line.strip_prefix("### `"))
        .filter_map(|rest| rest.split(['`', ' ']).next())
        .map(str::to_owned)
        .collect()
}

/// Commands named by the first cell of a README command-table row.
fn readme_commands(readme: &str) -> BTreeSet<String> {
    readme
        .lines()
        .filter_map(|line| line.strip_prefix("| `"))
        .filter_map(|rest| rest.split(['`', ' ']).next())
        .map(str::to_owned)
        .collect()
}

#[test]
fn every_subcommand_is_documented_and_no_documented_command_is_stale() {
    let root = repo_root();
    let docs = std::fs::read_to_string(root.join("docs/cli.md")).unwrap();
    let readme = std::fs::read_to_string(root.join("README.md")).unwrap();

    let commands = subcommands();
    let documented = documented_commands(&docs);
    let tabled = readme_commands(&readme);

    let missing_sections: Vec<_> = commands.difference(&documented).cloned().collect();
    assert!(
        missing_sections.is_empty(),
        "subcommands without a `###` section in docs/cli.md: {missing_sections:?}"
    );

    let missing_rows: Vec<_> = commands.difference(&tabled).cloned().collect();
    assert!(
        missing_rows.is_empty(),
        "subcommands without a README command-table row: {missing_rows:?}"
    );

    let stale: Vec<_> = documented.difference(&commands).cloned().collect();
    assert!(
        stale.is_empty(),
        "docs/cli.md documents commands the CLI does not expose: {stale:?}"
    );
}
