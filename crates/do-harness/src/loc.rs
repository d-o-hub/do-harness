//! `do-harness loc`: line-of-code state for the 500-LOC invariant.
//!
//! The ceiling is enforced by `scripts/check-loc.sh`, a sensor that only fires
//! after code exists. This command is the feedforward half: it reports each
//! file's distance to the ceiling and how much of it is fixtures, so the
//! question "is this file about to become a problem, and where do I cut it?"
//! is answerable without running the sensor or reading the file.
//!
//! Scope matches the sensor: every `.rs` file under `<root>/crates`, minus
//! `target/`. The thresholds are duplicated from the script deliberately —
//! [`MAX_LINES`] and [`WARN_THRESHOLD`] are the same numbers the shell sensor
//! uses, and a change to one must be made in both.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::Serialize;

use crate::report::Format;

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

/// Recursively collects `.rs` files under `dir`, skipping `target/`.
pub fn collect_rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        if kind.is_dir() {
            if entry.file_name() == "target" {
                continue;
            }
            collect_rust_files(&path, out);
        } else if kind.is_file() && path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
}

/// Measures every `.rs` file under `<root>/crates`, in path order.
///
/// # Errors
///
/// Returns an error when there is no `crates` directory or a file is
/// unreadable.
pub fn scan(root: &Path) -> Result<Vec<FileLocInfo>> {
    let base = root.join("crates");
    if !base.is_dir() {
        bail!("no crates directory under {}", root.display());
    }
    let mut files = Vec::new();
    collect_rust_files(&base, &mut files);
    files.sort();
    files.iter().map(|file| measure(root, file)).collect()
}

/// Measures explicitly named files and directories, in path order.
///
/// # Errors
///
/// Returns an error for a path that does not exist, or a directory holding no
/// `.rs` file: a query that silently measures nothing is worse than a failure.
pub fn collect_paths(root: &Path, paths: &[PathBuf]) -> Result<Vec<FileLocInfo>> {
    let mut files = Vec::new();
    for path in paths {
        if path.is_dir() {
            let mut found = Vec::new();
            collect_rust_files(path, &mut found);
            if found.is_empty() {
                bail!("no .rs files under {}", path.display());
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
pub fn run(root: &Path, format: Format, warn_only: bool, paths: &[PathBuf]) -> Result<()> {
    let mut infos = if paths.is_empty() {
        scan(root)?
    } else {
        collect_paths(root, paths)?
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
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use std::fmt::Write;

    use super::*;

    /// A file body of `preamble` lines followed by an inline test module.
    fn with_test_module(preamble: usize, tests: usize) -> String {
        let mut text = String::new();
        for i in 0..preamble {
            writeln!(text, "fn f{i}() {{}}").unwrap();
        }
        text.push_str("#[cfg(test)]\nmod tests {\n");
        for i in 0..tests {
            writeln!(text, "    fn t{i}() {{}}").unwrap();
        }
        text.push_str("}\n");
        text
    }

    #[test]
    fn split_counts_marker_line_as_test() {
        let text = with_test_module(10, 3);
        let info = measure_text("x.rs".to_owned(), &text);
        assert_eq!(
            info.lines, 16,
            "10 code + cfg(test) + mod + 3 tests + brace"
        );
        assert_eq!(info.code_lines, Some(10));
        assert_eq!(info.test_lines, Some(6));
        assert_eq!(
            info.code_lines.unwrap() + info.test_lines.unwrap(),
            info.lines
        );
        assert_eq!(test_marker_line(&text), Some(11));
    }

    #[test]
    fn file_without_test_module_has_no_split() {
        let info = measure_text("x.rs".to_owned(), "fn a() {}\nfn b() {}\n");
        assert_eq!(info.code_lines, None);
        assert_eq!(info.test_lines, None);
        assert_eq!(test_marker_line("fn a() {}\nfn b() {}\n"), None);
    }

    /// The ceiling itself is still OK; the threshold is inclusive.
    #[test]
    fn status_bands_are_inclusive_at_thresholds() {
        assert_eq!(LocStatus::of(WARN_THRESHOLD - 1), LocStatus::Ok);
        assert_eq!(LocStatus::of(WARN_THRESHOLD), LocStatus::Warn);
        assert_eq!(LocStatus::of(MAX_LINES), LocStatus::Warn);
        assert_eq!(LocStatus::of(MAX_LINES + 1), LocStatus::Fail);
    }

    #[test]
    fn scan_covers_crates_only_and_skips_target() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join("crates/app/src")).unwrap();
        std::fs::create_dir_all(root.join("crates/app/target/debug")).unwrap();
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(root.join("crates/app/src/lib.rs"), "fn a() {}\n").unwrap();
        std::fs::write(root.join("crates/app/target/debug/gen.rs"), "fn b() {}\n").unwrap();
        std::fs::write(root.join("src/outside.rs"), "fn c() {}\n").unwrap();
        std::fs::write(root.join("crates/app/README.md"), "noise\n").unwrap();

        let infos = scan(root).unwrap();
        let paths: Vec<&str> = infos.iter().map(|info| info.path.as_str()).collect();
        assert_eq!(paths, vec!["crates/app/src/lib.rs"]);
    }

    #[test]
    fn explicit_paths_are_measured_and_missing_paths_fail() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join("crates/app")).unwrap();
        std::fs::write(root.join("crates/app/lib.rs"), with_test_module(2, 1)).unwrap();
        std::fs::write(root.join("crates/app/other.rs"), "fn a() {}\n").unwrap();
        std::fs::write(root.join("crates/app/notes.txt"), "noise\n").unwrap();

        let infos = collect_paths(root, &[root.join("crates/app")]).unwrap();
        assert_eq!(
            infos.iter().map(|i| i.path.as_str()).collect::<Vec<_>>(),
            vec!["crates/app/lib.rs", "crates/app/other.rs"],
            "directories expand to .rs files only"
        );
        assert_eq!(infos[0].test_lines, Some(4));

        assert!(
            collect_paths(root, &[root.join("crates/missing")])
                .unwrap_err()
                .to_string()
                .contains("no such file or directory")
        );
    }
}
