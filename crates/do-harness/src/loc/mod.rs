//! `do-harness loc`: line-of-code state for the 500-LOC invariant.
//!
//! The ceiling is enforced by `scripts/check-loc.sh`, a sensor that only fires
//! after code exists. This command is the feedforward half: it reports each
//! file's distance to the ceiling and how much of it is fixtures, so the
//! question "is this file about to become a problem, and where do I cut it?"
//! is answerable without running the sensor or reading the file.
//!
//! Scope matches the sensors: every `.rs` file under `<root>/crates` and
//! `<root>/src`, minus `target/`, which together cover this workspace and the
//! single-crate layout `init` scaffolds. The thresholds are duplicated from the
//! script deliberately — [`MAX_LINES`] and [`WARN_THRESHOLD`] are the same
//! numbers the shell sensor uses, and a change to one must be made in both.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::Serialize;

use crate::report::Format;

mod scope;

#[allow(unused_imports)]
pub use scope::{LocScope, PRUNE_NAMES, parse_exts, parse_roots, parse_sensor_argv};

/// Hard per-file ceiling, matching `MAX` in `scripts/check-loc.sh`.
pub const MAX_LINES: usize = 500;

/// Decomposition threshold, matching `THRESHOLD` in `scripts/check-loc.sh`.
pub const WARN_THRESHOLD: usize = 450;

/// Band a file's line count falls into.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum LocStatus {
    /// Below the decomposition threshold.
    Ok,
    /// At or above the threshold but at or below the ceiling.
    Warn,
    /// Above the ceiling: the sensor fails.
    Fail,
}

impl LocStatus {
    /// Classifies a line count, where the ceiling itself still counts as OK.
    #[must_use]
    fn of(lines: usize) -> Self {
        if lines > MAX_LINES {
            Self::Fail
        } else if lines >= WARN_THRESHOLD {
            Self::Warn
        } else {
            Self::Ok
        }
    }
}

/// Measured size of one Rust source file.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FileLocInfo {
    /// Repository-relative path with forward slashes.
    pub path: String,
    /// Total line count.
    pub lines: usize,
    /// Ceiling this count is measured against.
    pub max: usize,
    /// Production lines before the inline `#[cfg(test)]` module, when present.
    pub code_lines: Option<usize>,
    /// Fixture lines from the `#[cfg(test)]` marker to EOF, when present.
    pub test_lines: Option<usize>,
    /// Band derived from `lines`.
    pub status: LocStatus,
}

/// Uppercase band label used in text output.
#[must_use]
fn status_label(status: LocStatus) -> &'static str {
    match status {
        LocStatus::Ok => "OK",
        LocStatus::Warn => "WARN",
        LocStatus::Fail => "FAIL",
    }
}

/// Returns the 1-based line of the first `#[cfg(test)]` marker in `text`.
///
/// Matches the shell sensor's pattern (`^[[:space:]]*#\[cfg\(test\)\]`), so
/// the Rust and shell code/test splits agree on the same file.
#[must_use]
pub fn test_marker_line(text: &str) -> Option<usize> {
    text.lines()
        .position(|line| line.trim_start().starts_with("#[cfg(test)]"))
        .map(|index| index + 1)
}

/// Measures source text that was already read, using `display` as its path.
///
/// Lines are counted as `text.lines().count()`, which equals `wc -l` for the
/// newline-terminated sources `cargo fmt` guarantees and never undercounts the
/// sensor's number otherwise.
#[must_use]
pub fn measure_text(display: String, text: &str) -> FileLocInfo {
    let lines = text.lines().count();
    let (code_lines, test_lines) = match test_marker_line(text) {
        Some(marker) => {
            let code = marker - 1;
            (Some(code), Some(lines - code))
        }
        None => (None, None),
    };
    FileLocInfo {
        path: display,
        lines,
        max: MAX_LINES,
        code_lines,
        test_lines,
        status: LocStatus::of(lines),
    }
}

/// Repository-relative display path with forward slashes.
#[must_use]
pub fn display_path(root: &Path, file: &Path) -> String {
    file.strip_prefix(root)
        .unwrap_or(file)
        .to_string_lossy()
        .replace('\\', "/")
}

/// Reads and measures one file.
///
/// # Errors
///
/// Returns an error when the file cannot be read as UTF-8 text.
pub fn measure(root: &Path, file: &Path) -> Result<FileLocInfo> {
    let text = std::fs::read_to_string(file)
        .with_context(|| format!("failed to read {}", file.display()))?;
    Ok(measure_text(display_path(root, file), &text))
}

/// Directories the default Rust LOC invariant covers, relative to the workspace root.
pub const SCOPE_DIRS: [&str; 2] = ["crates", "src"];

/// True when `file` lies inside a directory [`scan`] default measures.
///
/// Callers that mutate or index Rust sources use this so they accept exactly
/// the files the sensors can fail on: the workspace layout (`crates/`) and the
/// single-crate layout `init` scaffolds (`src/`).
#[must_use]
pub fn in_scope(root: &Path, file: &Path) -> bool {
    SCOPE_DIRS
        .iter()
        .any(|dir| file.starts_with(root.join(dir)))
}

/// Recursively collects source files under `dir` matching `exts`, skipping `PRUNE_NAMES`.
pub fn collect_source_files(dir: &Path, exts: &[String], out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        if kind.is_dir() {
            if let Some(name) = entry.file_name().to_str() {
                if PRUNE_NAMES.contains(&name) {
                    continue;
                }
            }
            collect_source_files(&path, exts, out);
        } else if kind.is_file() {
            if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                if exts.iter().any(|target| target.eq_ignore_ascii_case(ext)) {
                    out.push(path);
                }
            }
        }
    }
}

/// Recursively collects `.rs` files under `dir`, skipping `target/`.
#[allow(dead_code)]
pub fn collect_rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    collect_source_files(dir, &["rs".to_string()], out);
}

/// Measures source files matching `scope` under `<root>`.
///
/// # Errors
///
/// Returns an error when explicit roots do not exist, when no source files are found,
/// or when a file cannot be read.
pub fn scan(root: &Path, scope: &LocScope) -> Result<Vec<FileLocInfo>> {
    if scope.roots_explicit {
        for dir in &scope.roots {
            let base = root.join(dir);
            if !base.is_dir() {
                bail!("--root {dir} does not exist under {}", root.display());
            }
        }
    }

    let mut files = Vec::new();
    for dir in &scope.roots {
        let base = root.join(dir);
        if base.is_dir() {
            collect_source_files(&base, &scope.exts, &mut files);
        }
    }

    if files.is_empty() {
        let roots_str = scope
            .roots
            .iter()
            .map(|r| format!("{}/{r}", root.display()))
            .collect::<Vec<_>>()
            .join(" or ");
        bail!(
            "no source files matching extension(s) ({}) under {}; pass explicit file/directory paths or use --root / --ext",
            scope.exts.join(", "),
            roots_str
        );
    }
    files.sort();
    files.dedup();
    files.iter().map(|file| measure(root, file)).collect()
}

/// Measures explicitly named files and directories, in path order.
///
/// # Errors
///
/// Returns an error for a path that does not exist, or a directory holding no
/// matching source files.
pub fn collect_paths(root: &Path, paths: &[PathBuf], exts: &[String]) -> Result<Vec<FileLocInfo>> {
    let mut files = Vec::new();
    for path in paths {
        if path.is_dir() {
            let mut found = Vec::new();
            collect_source_files(path, exts, &mut found);
            if found.is_empty() {
                bail!(
                    "no files matching extension(s) ({}) under {}",
                    exts.join(", "),
                    path.display()
                );
            }
            files.extend(found);
        } else if path.is_file() {
            files.push(path.clone());
        } else {
            bail!("no such file or directory: {}", path.display());
        }
    }
    files.sort();
    files.dedup();
    files.iter().map(|file| measure(root, file)).collect()
}

/// Runs `do-harness loc`.
///
/// The command is a query: it always reports and never fails on an over-limit
/// file, because the `loc` sensor owns that gate. With no arguments it
/// measures the whole workspace scope; with `--warn` it narrows to the
/// decomposition band, and with explicit paths it reports exactly those.
///
/// # Errors
///
/// Returns an error when the scope cannot be read (see [`scan`] and
/// [`collect_paths`]).
pub fn run(
    root: &Path,
    config_path: Option<&Path>,
    format: Format,
    warn_only: bool,
    cli_roots: &[String],
    cli_exts: &[String],
    paths: &[PathBuf],
) -> Result<()> {
    let scope = LocScope::resolve(root, config_path, cli_roots, cli_exts);
    let mut infos = if paths.is_empty() {
        scan(root, &scope)?
    } else {
        collect_paths(root, paths, &scope.exts)?
    };
    let scanned = infos.len();
    if warn_only {
        infos.retain(|info| info.status != LocStatus::Ok);
    }
    infos.sort_by(|a, b| b.lines.cmp(&a.lines).then_with(|| a.path.cmp(&b.path)));

    match format {
        Format::Text => {
            for info in &infos {
                let split = match (info.code_lines, info.test_lines) {
                    (Some(code), Some(test)) => format!("  [code={code} test={test}]"),
                    _ => String::new(),
                };
                println!(
                    "{}  {}/{}  {}{split}",
                    info.path,
                    info.lines,
                    info.max,
                    status_label(info.status)
                );
            }
            if warn_only && infos.is_empty() {
                println!("OK: no file at or above {WARN_THRESHOLD} lines ({scanned} scanned).");
            }
        }
        Format::Json => println!("{}", serde_json::to_string(&infos)?),
    }
    Ok(())
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
