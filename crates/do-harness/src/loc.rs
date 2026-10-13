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

/// Directory names pruned during source discovery (matching `check-loc.sh`).
pub const PRUNE_NAMES: &[&str] = &[
    "node_modules",
    "target",
    "dist",
    "build",
    "coverage",
    "vendor",
    ".next",
    "out",
    ".git",
];

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

/// Normalizes comma-separated roots, stripping leading `./` and trailing `/`.
#[must_use]
pub fn parse_roots(raw: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    for item in raw {
        for part in item.split(',') {
            let trimmed = part.trim();
            if !trimmed.is_empty() {
                let norm = trimmed.strip_prefix("./").unwrap_or(trimmed);
                let norm = norm.trim_end_matches('/');
                if !norm.is_empty() && !out.iter().any(|r| r == norm) {
                    out.push(norm.to_owned());
                }
            }
        }
    }
    out
}

/// Normalizes comma-separated extensions, stripping leading `.`.
#[must_use]
pub fn parse_exts(raw: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    for item in raw {
        for part in item.split(',') {
            let trimmed = part.trim();
            if !trimmed.is_empty() {
                let norm = trimmed.strip_prefix('.').unwrap_or(trimmed);
                if !norm.is_empty() && !out.iter().any(|e| e == norm) {
                    out.push(norm.to_owned());
                }
            }
        }
    }
    out
}

/// Extracts `--root` and `--ext` values from `argv` (e.g. `check-loc.sh --root web,src --ext ts,tsx`).
#[must_use]
pub fn parse_sensor_argv(argv: &[String]) -> (Vec<String>, Vec<String>) {
    let mut roots = Vec::new();
    let mut exts = Vec::new();
    let mut iter = argv.iter().peekable();
    while let Some(arg) = iter.next() {
        if arg == "--root" {
            if let Some(val) = iter.next() {
                roots.push(val.clone());
            }
        } else if let Some(val) = arg.strip_prefix("--root=") {
            roots.push(val.to_owned());
        } else if arg == "--ext" {
            if let Some(val) = iter.next() {
                exts.push(val.clone());
            }
        } else if let Some(val) = arg.strip_prefix("--ext=") {
            exts.push(val.to_owned());
        }
    }
    (parse_roots(&roots), parse_exts(&exts))
}

/// Resolved discovery scope for `loc`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocScope {
    pub roots: Vec<String>,
    pub exts: Vec<String>,
    pub roots_explicit: bool,
}

impl LocScope {
    /// Resolves roots and extensions given CLI overrides, config path, and repository root.
    #[must_use]
    pub fn resolve(
        root: &Path,
        config_path: Option<&Path>,
        cli_roots: &[String],
        cli_exts: &[String],
    ) -> Self {
        let parsed_cli_roots = parse_roots(cli_roots);
        let parsed_cli_exts = parse_exts(cli_exts);

        let (config_language, sensor_roots, sensor_exts) = load_config_info(root, config_path);

        let roots_explicit = !parsed_cli_roots.is_empty() || !sensor_roots.is_empty();

        let roots = if !parsed_cli_roots.is_empty() {
            parsed_cli_roots
        } else if !sensor_roots.is_empty() {
            sensor_roots
        } else if matches!(
            config_language.as_deref(),
            Some("generic" | "node" | "web")
        ) {
            vec![
                "packages".to_owned(),
                "apps".to_owned(),
                "src".to_owned(),
                "crates".to_owned(),
            ]
        } else {
            vec!["src".to_owned(), "crates".to_owned()]
        };

        let exts = if !parsed_cli_exts.is_empty() {
            parsed_cli_exts
        } else if !sensor_exts.is_empty() {
            sensor_exts
        } else if matches!(
            config_language.as_deref(),
            Some("generic" | "node" | "web")
        ) {
            vec![
                "ts".to_owned(),
                "tsx".to_owned(),
                "js".to_owned(),
                "jsx".to_owned(),
                "rs".to_owned(),
            ]
        } else {
            vec!["rs".to_owned()]
        };

        Self {
            roots,
            exts,
            roots_explicit,
        }
    }
}

/// Helper reading language and `loc` sensor argv from `do-harness.toml`.
fn load_config_info(root: &Path, explicit: Option<&Path>) -> (Option<String>, Vec<String>, Vec<String>) {
    let path = match explicit {
        Some(path) => path.to_path_buf(),
        None => root.join("do-harness.toml"),
    };
    let Ok(text) = std::fs::read_to_string(&path) else {
        return (None, Vec::new(), Vec::new());
    };
    let Ok(cfg) = toml::from_str::<crate::config::Config>(&text) else {
        return (None, Vec::new(), Vec::new());
    };

    let mut sensor_roots = Vec::new();
    let mut sensor_exts = Vec::new();
    for sensor in cfg.effective_sensors() {
        if sensor.name == "loc" {
            let (r, e) = parse_sensor_argv(&sensor.argv);
            sensor_roots.extend(r);
            sensor_exts.extend(e);
        }
    }

    (cfg.language, sensor_roots, sensor_exts)
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
pub fn collect_paths(
    root: &Path,
    paths: &[PathBuf],
    exts: &[String],
) -> Result<Vec<FileLocInfo>> {
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

    /// `loc` must cover everything either shipped `check-loc.sh` scans, so a
    /// scaffolded single-crate repo (`src/`) and this workspace (`crates/`) are
    /// both reported, with `target/` and non-Rust files excluded.
    #[test]
    fn scan_covers_crates_and_src_but_skips_target() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join("crates/app/src")).unwrap();
        std::fs::create_dir_all(root.join("crates/app/target/debug")).unwrap();
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(root.join("crates/app/src/lib.rs"), "fn a() {}\n").unwrap();
        std::fs::write(root.join("crates/app/target/debug/gen.rs"), "fn b() {}\n").unwrap();
        std::fs::write(root.join("src/outside.rs"), "fn c() {}\n").unwrap();
        std::fs::write(root.join("crates/app/README.md"), "noise\n").unwrap();

        let scope = LocScope::resolve(root, None, &[], &[]);
        let infos = scan(root, &scope).unwrap();
        let paths: Vec<&str> = infos.iter().map(|info| info.path.as_str()).collect();
        assert_eq!(paths, vec!["crates/app/src/lib.rs", "src/outside.rs"]);
    }

    /// A scaffolded single-crate repo has no `crates/` at all: `loc` must still
    /// measure it rather than failing.
    #[test]
    fn scan_works_without_a_crates_directory() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(root.join("src/main.rs"), "fn main() {}\n").unwrap();

        let scope = LocScope::resolve(root, None, &[], &[]);
        let infos = scan(root, &scope).unwrap();
        assert_eq!(infos.len(), 1);
        assert_eq!(infos[0].path, "src/main.rs");
    }

    #[test]
    fn scan_fails_when_no_sources_exist() {
        let dir = tempfile::tempdir().unwrap();
        let scope = LocScope::resolve(dir.path(), None, &[], &[]);
        let err = scan(dir.path(), &scope).unwrap_err().to_string();
        assert!(err.contains("no source files matching"), "{err}");
    }

    #[test]
    fn explicit_paths_are_measured_and_missing_paths_fail() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join("crates/app")).unwrap();
        std::fs::write(root.join("crates/app/lib.rs"), with_test_module(2, 1)).unwrap();
        std::fs::write(root.join("crates/app/other.rs"), "fn a() {}\n").unwrap();
        std::fs::write(root.join("crates/app/notes.txt"), "noise\n").unwrap();

        let scope = LocScope::resolve(root, None, &[], &[]);
        let infos = collect_paths(root, &[root.join("crates/app")], &scope.exts).unwrap();
        assert_eq!(
            infos.iter().map(|i| i.path.as_str()).collect::<Vec<_>>(),
            vec!["crates/app/lib.rs", "crates/app/other.rs"],
            "directories expand to matching files only"
        );
        assert_eq!(infos[0].test_lines, Some(4));

        assert!(
            collect_paths(root, &[root.join("crates/missing")], &scope.exts)
                .unwrap_err()
                .to_string()
                .contains("no such file or directory")
        );
    }

    #[test]
    fn scan_generic_workspace_discovers_packages_and_apps() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join("packages/contracts/src")).unwrap();
        std::fs::create_dir_all(root.join("apps/web/src")).unwrap();
        std::fs::create_dir_all(root.join("packages/contracts/node_modules")).unwrap();
        std::fs::write(root.join("do-harness.toml"), "language = \"generic\"\n").unwrap();
        std::fs::write(
            root.join("packages/contracts/src/evidence-framing.ts"),
            "export const x = 1;\n",
        )
        .unwrap();
        std::fs::write(
            root.join("apps/web/src/index.tsx"),
            "export const App = () => null;\n",
        )
        .unwrap();
        std::fs::write(
            root.join("packages/contracts/node_modules/noise.ts"),
            "export const noise = 2;\n",
        )
        .unwrap();

        let scope = LocScope::resolve(root, None, &[], &[]);
        assert_eq!(scope.roots, vec!["packages", "apps", "src", "crates"]);
        assert_eq!(scope.exts, vec!["ts", "tsx", "js", "jsx", "rs"]);

        let infos = scan(root, &scope).unwrap();
        let paths: Vec<&str> = infos.iter().map(|info| info.path.as_str()).collect();
        assert_eq!(
            paths,
            vec![
                "apps/web/src/index.tsx",
                "packages/contracts/src/evidence-framing.ts"
            ]
        );
    }

    #[test]
    fn cli_root_and_ext_overrides_win() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join("web/src")).unwrap();
        std::fs::write(root.join("web/src/app.ts"), "const a = 1;\n").unwrap();
        std::fs::write(root.join("web/src/ignored.rs"), "fn f() {}\n").unwrap();

        let scope = LocScope::resolve(
            root,
            None,
            &["./web/src/".to_string()],
            &[".ts".to_string()],
        );
        assert_eq!(scope.roots, vec!["web/src"]);
        assert_eq!(scope.exts, vec!["ts"]);
        assert!(scope.roots_explicit);

        let infos = scan(root, &scope).unwrap();
        assert_eq!(infos.len(), 1);
        assert_eq!(infos[0].path, "web/src/app.ts");
    }

    #[test]
    fn parses_sensor_argv_roots_and_exts() {
        let argv = vec![
            "bash".to_owned(),
            "scripts/check-loc.sh".to_owned(),
            "--root".to_owned(),
            "web,src,crates".to_owned(),
            "--ext".to_owned(),
            "rs,ts,tsx".to_owned(),
        ];
        let (roots, exts) = parse_sensor_argv(&argv);
        assert_eq!(roots, vec!["web", "src", "crates"]);
        assert_eq!(exts, vec!["rs", "ts", "tsx"]);
    }
}
