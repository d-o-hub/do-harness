//! `do-harness lint`: the feedforward-guide linter.
//!
//! `verify` proves the feedback layer (sensors). `AGENTS.md` and the skills are
//! the feedforward layer, and nothing validated them: a routing section could
//! name a skill that does not exist, two layered contracts could repeat the
//! same instruction, and a skill could claim a check no sensor runs. This
//! command reads the guide layer and reports those defects without executing
//! anything.
//!
//! Exit codes: `0` when clean or warnings only, `1` when any error is
//! reported, `2` for a usage or configuration failure (including a missing
//! `AGENTS.md`, which means the command was pointed at the wrong root).

use std::path::{Path, PathBuf};

use anyhow::Result;
use serde::Serialize;

use crate::CliError;
use crate::config;
use crate::report::Format;

/// One diagnostic from the guide layer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Diagnostic {
    /// `error` | `warning`.
    pub severity: &'static str,
    /// Stable machine-readable code.
    pub code: &'static str,
    /// File the finding lives in, relative to the workspace root.
    pub path: String,
    /// 1-based line number, or `0` for a file-level finding.
    pub line: usize,
    /// Human-readable explanation.
    pub message: String,
}

impl Diagnostic {
    fn error(code: &'static str, path: &Path, line: usize, message: String) -> Self {
        Self {
            severity: "error",
            code,
            path: path.display().to_string(),
            line,
            message,
        }
    }

    fn warning(code: &'static str, path: &Path, line: usize, message: String) -> Self {
        Self {
            severity: "warning",
            code,
            path: path.display().to_string(),
            line,
            message,
        }
    }

    fn is_error(&self) -> bool {
        self.severity == "error"
    }
}

/// The machine-readable lint report (`--format json`).
#[derive(Debug, Clone, Serialize)]
pub struct LintReport {
    /// Workspace root the lint read.
    pub root: String,
    /// Every finding, in file/line order.
    pub diagnostics: Vec<Diagnostic>,
    /// Count of `severity == "error"` findings.
    pub errors: usize,
    /// Count of `severity == "warning"` findings.
    pub warnings: usize,
    /// True when there is at least one error (the exit-1 condition).
    pub ok: bool,
}

/// Runs `do-harness lint`.
///
/// # Errors
///
/// Returns a usage error when the root has no `AGENTS.md`, and a config error
/// when `do-harness.toml` cannot be read.
pub async fn run(root: &Path, format: Format) -> std::result::Result<(), CliError> {
    let report = lint(root).await.map_err(CliError::Usage)?;
    match format {
        Format::Text => print_text(&report),
        Format::Json => {
            println!(
                "{}",
                crate::report::json_pretty(&report).map_err(CliError::Verify)?
            );
        }
    }
    if report.ok {
        Ok(())
    } else {
        Err(CliError::Verify(anyhow::anyhow!(
            "{} guide error(s); see the diagnostics above",
            report.errors
        )))
    }
}

/// Collects every finding for a workspace.
///
/// # Errors
///
/// Returns an error when `AGENTS.md` is absent (a lint over nothing would
/// report `ok` for a guide layer that does not exist) or the config cannot be
/// loaded.
pub async fn lint(root: &Path) -> Result<LintReport> {
    let agents = guides(root)?;
    if agents.is_empty() {
        anyhow::bail!(
            "no AGENTS.md found at {}; run `do-harness init` or point --root at the workspace",
            root.display()
        );
    }
    let cfg = config::load(root, None).await?;
    let mut diagnostics = Vec::new();
    for path in &agents {
        let text = std::fs::read_to_string(path)?;
        let relative = path.strip_prefix(root).unwrap_or(path);
        broken_skill_references(&text, root, relative, &mut diagnostics);
        duplicate_instructions(&text, relative, &mut diagnostics);
    }
    uncovered_skills(root, &cfg, &mut diagnostics);
    diagnostics.sort_by(|left, right| {
        (&left.path, left.line, left.code).cmp(&(&right.path, right.line, right.code))
    });
    let errors = diagnostics.iter().filter(|item| item.is_error()).count();
    Ok(LintReport {
        root: root.display().to_string(),
        warnings: diagnostics.len() - errors,
        errors,
        ok: errors == 0,
        diagnostics,
    })
}

/// Every `AGENTS.md` under the root, skipping vendored and state trees.
fn guides(root: &Path) -> Result<Vec<PathBuf>> {
    let mut found = Vec::new();
    collect_guides(root, &mut found)?;
    found.sort();
    Ok(found)
}

fn collect_guides(dir: &Path, found: &mut Vec<PathBuf>) -> Result<()> {
    const SKIP: [&str; 5] = ["target", ".git", "vendor", "node_modules", ".do-harness"];
    if dir != dir.parent().unwrap_or(dir) {
        let name = dir
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("");
        if SKIP.contains(&name) {
            return Ok(());
        }
    }
    let agents = dir.join("AGENTS.md");
    if agents.is_file() {
        found.push(agents);
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Ok(());
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() && !path.is_symlink() {
            collect_guides(&path, found)?;
        }
    }
    Ok(())
}

/// Reports skill references whose target does not exist.
///
/// Two spellings are recognised: a markdown link to `.agents/skills/<name>`
/// (with or without `SKILL.md`), and the `skill://<name>` URI the runtime uses.
/// A reference to a skill-directory file other than `SKILL.md` must exist at
/// that exact path.
fn broken_skill_references(
    text: &str,
    root: &Path,
    relative: &Path,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for (index, line) in text.lines().enumerate() {
        let line_no = index + 1;
        for reference in skill_references(line) {
            let target = root.join(&reference);
            if !target.exists() {
                diagnostics.push(Diagnostic::error(
                    "broken-skill-reference",
                    relative,
                    line_no,
                    format!("references {reference}, which does not exist"),
                ));
            } else if target.is_dir() && !target.join("SKILL.md").is_file() {
                diagnostics.push(Diagnostic::error(
                    "skill-without-contract",
                    relative,
                    line_no,
                    format!("{reference} has no SKILL.md"),
                ));
            }
        }
    }
}

/// Extracts `.agents/skills/...` paths and `skill://<name>` URIs from a line.
fn skill_references(line: &str) -> Vec<String> {
    let mut references = Vec::new();
    for token in line
        .split(|character: char| character.is_whitespace() || "()[]`'\"<>|,".contains(character))
    {
        if let Some(name) = token.strip_prefix("skill://") {
            let name = name.trim_end_matches('/');
            if !name.is_empty() {
                references.push(format!(".agents/skills/{name}/SKILL.md"));
            }
            continue;
        }
        let trimmed = token.trim_end_matches(['.', ';', ':']);
        if trimmed.starts_with(".agents/skills/") && !references.iter().any(|r| r == trimmed) {
            references.push(trimmed.to_owned());
        }
    }
    references
}

/// Warns when two guide files repeat the same substantive instruction.
///
/// A repeated line is not always wrong (a short rule can bear restating), so
/// the check only reports lines with real content: at least 40 characters and
/// not a heading, fence, table, or bare path.
fn duplicate_instructions(text: &str, relative: &Path, diagnostics: &mut Vec<Diagnostic>) {
    let mut seen: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
    for (index, line) in text.lines().enumerate() {
        let instruction = line.trim();
        if instruction.len() < 40
            || instruction.starts_with(['#', '|', '>', '`', '-', '*'])
            || instruction.starts_with("<!--")
        {
            continue;
        }
        if let Some(first) = seen.get(instruction) {
            diagnostics.push(Diagnostic::warning(
                "duplicate-instruction",
                relative,
                index + 1,
                format!("repeats the instruction first seen on line {first}"),
            ));
        } else {
            seen.insert(instruction, index + 1);
        }
    }
}

/// Warns when a skill is not named by any configured sensor.
///
/// This is a heuristic, and deliberately a warning: a sensor may cover a skill
/// without naming it (a broad `cargo test` covers every guide claim), and a
/// skill may be intentionally advisory. The signal is "no sensor mentions this
/// skill at all", which is what a scaffolded guide drifting from the feedback
/// layer looks like.
fn uncovered_skills(root: &Path, cfg: &config::Config, diagnostics: &mut Vec<Diagnostic>) {
    let skills = root.join(".agents/skills");
    let Ok(entries) = std::fs::read_dir(&skills) else {
        return;
    };
    let mut haystack = String::new();
    for sensor in cfg.effective_sensors() {
        haystack.push_str(&sensor.name);
        haystack.push('\n');
        haystack.push_str(&sensor.argv.join(" "));
        haystack.push('\n');
        haystack.push_str(&sensor.when_changed.join(" "));
        haystack.push('\n');
    }
    for entry in entries.flatten() {
        if !entry.path().is_dir() {
            continue;
        }
        let Some(name) = entry.file_name().to_str().map(ToOwned::to_owned) else {
            continue;
        };
        let contract = entry.path().join("SKILL.md");
        if !contract.is_file() {
            diagnostics.push(Diagnostic::error(
                "skill-without-contract",
                Path::new(".agents/skills").join(&name).as_path(),
                0,
                "skill directory has no SKILL.md".to_owned(),
            ));
            continue;
        }
        if !haystack.contains(&name) {
            diagnostics.push(Diagnostic::warning(
                "skill-without-sensor",
                Path::new(".agents/skills")
                    .join(&name)
                    .join("SKILL.md")
                    .as_path(),
                0,
                "no configured sensor names this skill or its directory".to_owned(),
            ));
        }
    }
}

fn print_text(report: &LintReport) {
    for diagnostic in &report.diagnostics {
        let location = if diagnostic.line == 0 {
            diagnostic.path.clone()
        } else {
            format!("{}:{}", diagnostic.path, diagnostic.line)
        };
        println!(
            "{} [{}] {}: {}",
            diagnostic.severity.to_uppercase(),
            diagnostic.code,
            location,
            diagnostic.message
        );
    }
    println!("{} error(s), {} warning(s)", report.errors, report.warnings);
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    #[test]
    fn skill_references_find_both_spellings() {
        let refs = skill_references("see skill://harness and `.agents/skills/pr-triage/SKILL.md`.");
        assert_eq!(
            refs,
            vec![
                ".agents/skills/harness/SKILL.md".to_owned(),
                ".agents/skills/pr-triage/SKILL.md".to_owned(),
            ]
        );
    }

    #[test]
    fn duplicate_lines_become_warnings_and_short_ones_do_not() {
        let text = "This sentence is long enough to count as an instruction.\nThis sentence is long enough to count as an instruction.\nshort\n";
        let mut diagnostics = Vec::new();
        duplicate_instructions(text, Path::new("AGENTS.md"), &mut diagnostics);
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].code, "duplicate-instruction");
        assert_eq!(diagnostics[0].line, 2);
    }

    #[test]
    fn headings_and_tables_are_not_instructions() {
        let text = "# This heading is long enough to be mistaken for an instruction\n| a | b |\n";
        let mut diagnostics = Vec::new();
        duplicate_instructions(text, Path::new("AGENTS.md"), &mut diagnostics);
        assert_eq!(diagnostics.len(), 0);
    }
}
