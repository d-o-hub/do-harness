//! The maintenance check: generated blocks versus committed files.
//!
//! `cargo test` runs [`generated_docs_are_in_sync`] in check mode: every
//! generated region is compared byte-for-byte (after newline normalization)
//! with what the CLI renders right now, so a deleted section, a stale row, or
//! an option that moved to another command fails the sensor. Setting
//! `DO_HARNESS_DOCS_WRITE=1` rewrites the regions in place; CI refuses to
//! write, and structurally missing or obsolete regions are never inserted or
//! deleted automatically — the report names the exact section instead.

use std::fmt::Write as _;
use std::fs;
use std::path::Path;

use super::contract::{self, CliContract};
use super::markers;
use super::render;

/// Committed file holding the README command index.
pub const README_FILE: &str = "README.md";
/// Committed file holding the CLI reference.
pub const CLI_REFERENCE_FILE: &str = "docs/cli.md";
/// Committed machine-readable contract (consumed by the example checker).
pub const CONTRACT_FILE: &str = "tests/fixtures/cli-contract.json";
/// Region id of the README command index.
pub const README_REGION: &str = "cli-commands";
/// Region id of the global options block in the CLI reference.
pub const GLOBALS_REGION: &str = "cli-globals";
/// Region id prefix of the per-command blocks in the CLI reference.
pub const COMMAND_REGION_PREFIX: &str = "cli-contract ";
/// Environment variable that switches the maintenance test to update mode.
pub const WRITE_ENV: &str = "DO_HARNESS_DOCS_WRITE";
/// The exact command that repairs content drift.
pub const REPAIR_COMMAND: &str = "python3 scripts/docs.py sync --write";

/// One expected region (or whole file) and its rendered content.
#[derive(Debug, Clone)]
pub struct Expectation {
    /// Repository-relative file path.
    pub file: &'static str,
    /// Region id; `None` means the whole file is generated.
    pub region: Option<String>,
    /// The exact content the CLI renders today.
    pub expected: String,
}

/// Everything the CLI currently expects the committed artifacts to contain.
pub fn expectations(contract: &CliContract) -> Vec<Expectation> {
    let mut list = vec![
        Expectation {
            file: README_FILE,
            region: Some(README_REGION.to_string()),
            expected: render::readme_block(contract),
        },
        Expectation {
            file: CLI_REFERENCE_FILE,
            region: Some(GLOBALS_REGION.to_string()),
            expected: render::globals_block(contract),
        },
    ];
    for command in &contract.commands {
        list.push(Expectation {
            file: CLI_REFERENCE_FILE,
            region: Some(format!("{COMMAND_REGION_PREFIX}{}", command.path)),
            expected: render::command_block(command),
        });
    }
    list.push(Expectation {
        file: CONTRACT_FILE,
        region: None,
        expected: render::contract_json(contract),
    });
    list
}

/// One problem found in a committed artifact.
#[derive(Debug)]
pub enum Issue {
    /// The file does not exist under the repository root.
    MissingFile { file: String, detail: String },
    /// The marker pairs could not be parsed.
    Malformed { file: String, detail: String },
    /// An expected region is absent.
    MissingRegion { file: String, region: String },
    /// A region the CLI no longer produces is still committed.
    ExtraRegion { file: String, region: String },
    /// The committed content differs from the rendered content.
    Diff {
        file: String,
        region: Option<String>,
        expected: String,
        detail: String,
    },
}

impl Issue {
    /// Whether the update mode can repair this issue automatically.
    pub fn fixable(&self) -> bool {
        matches!(self, Self::Diff { .. })
    }

    fn describe(&self) -> String {
        match self {
            Self::MissingFile { file, detail } => format!("{file}: cannot be read ({detail})"),
            Self::Malformed { file, detail } => format!("{file}: {detail}"),
            Self::MissingRegion { file, region } => format!(
                "{file}: region `{region}` is missing — add the marker pair to the section documenting this command"
            ),
            Self::ExtraRegion { file, region } => format!(
                "{file}: region `{region}` is no longer produced by the CLI — delete its marker pair (and the section it documents)"
            ),
            Self::Diff {
                file,
                region: Some(region),
                detail,
                ..
            } => {
                format!("{file}: region `{region}` differs — {detail}")
            }
            Self::Diff {
                file,
                region: None,
                detail,
                ..
            } => format!("{file}: differs — {detail}"),
        }
    }
}

/// The result of one check run.
#[derive(Debug, Default)]
pub struct Report {
    /// Everything that does not match, in file order.
    pub issues: Vec<Issue>,
}

impl Report {
    /// True when the committed artifacts match the CLI.
    pub fn is_clean(&self) -> bool {
        self.issues.is_empty()
    }

    /// Human-readable report naming file, section, expected/actual, repair.
    pub fn message(&self) -> String {
        let mut out = format!(
            "generated documentation is out of date ({} issue(s)):\n",
            self.issues.len()
        );
        for issue in &self.issues {
            out.push_str("  ");
            out.push_str(&issue.describe());
            out.push('\n');
        }
        if self.issues.iter().any(Issue::fixable) {
            let _ = writeln!(out, "repair: {REPAIR_COMMAND}");
        }
        if self.issues.iter().any(|issue| !issue.fixable()) {
            out.push_str("structural issues (missing/obsolete regions) need the marker pair added or removed by hand first\n");
        }
        out
    }

    /// Applies the fixable content diffs; refuses on structural issues.
    pub fn apply(&self, root: &Path) -> Result<Vec<String>, String> {
        if self.issues.iter().any(|issue| !issue.fixable()) {
            return Err(self.message());
        }
        let mut changed = Vec::new();
        for issue in &self.issues {
            let Issue::Diff {
                file,
                region,
                expected,
                ..
            } = issue
            else {
                continue;
            };
            if let Some(region) = region {
                write_region(&root.join(file), region, expected)?;
                changed.push(format!("{file}: region `{region}`"));
            } else {
                write_whole_file(&root.join(file), expected)?;
                changed.push(file.clone());
            }
        }
        Ok(changed)
    }
}

/// Checks every committed artifact against the current CLI definitions.
pub fn check(root: &Path) -> Report {
    check_contract(root, &contract::build())
}

/// Checks against an explicitly supplied contract (fixtures reuse this).
pub fn check_contract(root: &Path, contract: &CliContract) -> Report {
    let expectations = expectations(contract);
    let mut files: Vec<&'static str> = Vec::new();
    for expectation in &expectations {
        if !files.contains(&expectation.file) {
            files.push(expectation.file);
        }
    }
    let mut report = Report::default();
    for file in files {
        let expected: Vec<&Expectation> = expectations
            .iter()
            .filter(|item| item.file == file)
            .collect();
        check_file(&mut report, root, file, &expected);
    }
    report
}

/// Checks one file: whole-file expectations, then regions, then obsolete ids.
fn check_file(report: &mut Report, root: &Path, file: &str, expected: &[&Expectation]) {
    let path = root.join(file);
    let text = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(err) => {
            report.issues.push(Issue::MissingFile {
                file: file.to_string(),
                detail: err.to_string(),
            });
            return;
        }
    };
    if let [only] = expected {
        if only.region.is_none() {
            if markers::normalize_eol(&text) != markers::normalize_eol(&only.expected) {
                report.issues.push(Issue::Diff {
                    file: file.to_string(),
                    region: None,
                    expected: only.expected.clone(),
                    detail: first_difference(&only.expected, &text),
                });
            }
            return;
        }
    }
    let regions = match markers::parse_regions(&text) {
        Ok(regions) => regions,
        Err(detail) => {
            report.issues.push(Issue::Malformed {
                file: file.to_string(),
                detail,
            });
            return;
        }
    };
    for expectation in expected {
        let Some(id) = expectation.region.as_deref() else {
            continue;
        };
        match regions.iter().find(|region| region.id == id) {
            Some(region) => {
                let actual = &text[region.content.clone()];
                if markers::normalize_eol(actual) != markers::normalize_eol(&expectation.expected) {
                    report.issues.push(Issue::Diff {
                        file: file.to_string(),
                        region: Some(id.to_string()),
                        expected: expectation.expected.clone(),
                        detail: first_difference(&expectation.expected, actual),
                    });
                }
            }
            None => report.issues.push(Issue::MissingRegion {
                file: file.to_string(),
                region: id.to_string(),
            }),
        }
    }
    for region in &regions {
        let expected_id = expected
            .iter()
            .any(|item| item.region.as_deref() == Some(region.id.as_str()));
        if is_generated_region(&region.id) && !expected_id {
            report.issues.push(Issue::ExtraRegion {
                file: file.to_string(),
                region: region.id.clone(),
            });
        }
    }
}

/// Region ids that only this generator may produce.
fn is_generated_region(id: &str) -> bool {
    id == README_REGION || id == GLOBALS_REGION || id.starts_with(COMMAND_REGION_PREFIX)
}

/// Describes the first differing line, with counts, newline-normalized.
fn first_difference(expected: &str, actual: &str) -> String {
    const MISSING: &str = "<no line>";
    let expected = markers::normalize_eol(expected);
    let actual = markers::normalize_eol(actual);
    let expected_lines: Vec<&str> = expected.lines().collect();
    let actual_lines: Vec<&str> = actual.lines().collect();
    let total = expected_lines.len().max(actual_lines.len());
    let differing: Vec<usize> = (0..total)
        .filter(|&index| {
            expected_lines.get(index).copied().unwrap_or(MISSING)
                != actual_lines.get(index).copied().unwrap_or(MISSING)
        })
        .collect();
    let Some(&first) = differing.first() else {
        return String::from("content matches after newline normalization");
    };
    let expected_line = expected_lines.get(first).copied().unwrap_or(MISSING);
    let actual_line = actual_lines.get(first).copied().unwrap_or(MISSING);
    format!(
        "line {} of {} differs ({}) — expected `{}`, actual `{}`",
        first + 1,
        total.max(1),
        match differing.len() {
            1 => String::from("1 line differs"),
            n => format!("{n} lines differ"),
        },
        truncate(expected_line),
        truncate(actual_line),
    )
}

/// Bounds one report line so a whole table does not flood the failure output.
fn truncate(line: &str) -> String {
    const LIMIT: usize = 140;
    if line.chars().count() <= LIMIT {
        line.to_string()
    } else {
        line.chars().take(LIMIT).collect::<String>() + "…"
    }
}

/// Rewrites one region, preserving the file's outside bytes and EOL style.
pub fn write_region(path: &Path, region_id: &str, content: &str) -> Result<(), String> {
    let text = fs::read_to_string(path).map_err(|err| format!("{}: {err}", path.display()))?;
    let regions =
        markers::parse_regions(&text).map_err(|detail| format!("{}: {detail}", path.display()))?;
    let region = regions
        .iter()
        .find(|region| region.id == region_id)
        .ok_or_else(|| format!("{}: region `{region_id}` not found", path.display()))?;
    if markers::normalize_eol(&text[region.content.clone()]) == markers::normalize_eol(content) {
        return Ok(());
    }
    let updated = markers::splice(&text, region, content);
    write_through_symlinks(path, &updated)
}

/// Rewrites a whole generated file (the JSON contract).
fn write_whole_file(path: &Path, content: &str) -> Result<(), String> {
    let text = fs::read_to_string(path).map_err(|err| format!("{}: {err}", path.display()))?;
    if markers::normalize_eol(&text) == markers::normalize_eol(content) {
        return Ok(());
    }
    write_through_symlinks(path, content)
}

/// Writes the resolved target so a symlinked artifact stays a symlink.
fn write_through_symlinks(path: &Path, content: &str) -> Result<(), String> {
    let target = fs::canonicalize(path).map_err(|err| format!("{}: {err}", path.display()))?;
    fs::write(&target, content).map_err(|err| format!("{}: {err}", target.display()))
}

/// The maintenance gate: check by default, update under [`WRITE_ENV`].
#[test]
fn generated_docs_are_in_sync() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let write = std::env::var(WRITE_ENV).is_ok_and(|value| !value.is_empty() && value != "0");
    assert!(
        !(write && ci_is_set()),
        "refusing to write generated documentation while CI is set"
    );
    let report = check(&root);
    if !write {
        assert!(report.is_clean(), "{}", report.message());
        return;
    }
    match report.apply(&root) {
        Ok(changed) if changed.is_empty() => println!("generated documentation is already in sync"),
        Ok(changed) => {
            println!("updated {} generated region(s):", changed.len());
            for item in &changed {
                println!("  {item}");
            }
            let verification = check(&root);
            assert!(
                verification.is_clean(),
                "post-write verification failed:\n{}",
                verification.message()
            );
        }
        Err(message) => panic!("{message}"),
    }
}

/// Whether the environment marks this run as CI (writes are refused there).
fn ci_is_set() -> bool {
    std::env::var("CI").is_ok_and(|value| !value.is_empty() && value != "0" && value != "false")
}
