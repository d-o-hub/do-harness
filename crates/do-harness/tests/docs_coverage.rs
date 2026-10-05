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

/// Runs `do-harness <path> --help` and returns the text.
fn help_text(path: &[String]) -> String {
    let output = Command::new(env!("CARGO_BIN_EXE_do-harness"))
        .args(path)
        .arg("--help")
        .output()
        .expect("run do-harness --help");
    assert!(output.status.success(), "--help must exit 0 for {path:?}");
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// Command names in a help text's `Commands:` section; `help` is not a
/// feature.
fn commands_section(help: &str) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    let mut in_commands = false;
    for line in help.lines() {
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
    names
}

/// The subcommands `--help` advertises.
fn subcommands() -> BTreeSet<String> {
    let text = help_text(&[]);
    let names = commands_section(&text);
    assert!(
        !names.is_empty(),
        "no subcommands parsed from --help:\n{text}"
    );
    names
}

/// Long flags advertised by any command's `--help`, walking nested
/// subcommands to depth two (`task add`, `pr review`).
fn help_flags() -> BTreeSet<String> {
    let mut flags = BTreeSet::new();
    let mut queue: Vec<Vec<String>> = vec![Vec::new()];
    let mut seen: BTreeSet<Vec<String>> = BTreeSet::new();
    while let Some(path) = queue.pop() {
        if path.len() > 2 || !seen.insert(path.clone()) {
            continue;
        }
        let text = help_text(&path);
        flags.extend(flag_entries(&text));
        for child in commands_section(&text) {
            let mut next = path.clone();
            next.push(child);
            queue.push(next);
        }
    }
    flags
}

/// Long flags from option-entry lines of clap `--help` output. clap's
/// built-in `--help`/`--version` are not feature flags.
fn flag_entries(help: &str) -> BTreeSet<String> {
    let mut flags = BTreeSet::new();
    for line in help.lines() {
        let indent = line.len() - line.trim_start().len();
        // Option entries are indented 2-6 columns; wrapped description lines
        // align much further right.
        if !(2..=6).contains(&indent) {
            continue;
        }
        let trimmed = line.trim_start();
        let Some(rest) = trimmed.strip_prefix('-') else {
            continue;
        };
        let long = match rest.strip_prefix('-') {
            Some(long) => Some(long),
            None => rest.split_once(", --").map(|(_, long)| long),
        };
        let Some(long) = long else { continue };
        let name: String = long
            .chars()
            .take_while(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == '-')
            .collect();
        if name.is_empty() || name == "help" || name == "version" {
            continue;
        }
        flags.insert(name);
    }
    flags
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

#[test]
fn every_long_flag_is_documented() {
    let root = repo_root();
    let docs = std::fs::read_to_string(root.join("docs/cli.md")).unwrap();

    let flags = help_flags();
    assert!(!flags.is_empty(), "no flags parsed from --help output");

    let undocumented: Vec<_> = flags
        .iter()
        .filter(|flag| !docs.contains(&format!("--{flag}")))
        .cloned()
        .collect();
    assert!(
        undocumented.is_empty(),
        "flags advertised by --help but missing from docs/cli.md: {undocumented:?}"
    );
}
