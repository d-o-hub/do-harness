//! Scope resolution and TOML configuration parsing for `do-harness loc`.

use std::path::Path;

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
        } else if matches!(config_language.as_deref(), Some("generic" | "node" | "web")) {
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
        } else if matches!(config_language.as_deref(), Some("generic" | "node" | "web")) {
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
fn load_config_info(
    root: &Path,
    explicit: Option<&Path>,
) -> (Option<String>, Vec<String>, Vec<String>) {
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
