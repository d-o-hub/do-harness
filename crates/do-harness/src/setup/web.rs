//! `do-harness setup web`: bootstrap the web audit pack's peer dependencies.
//!
//! The web-pack runners SKIP when `playwright`, `@axe-core/playwright`, or
//! `lighthouse` are missing from the adopting repository's `node_modules`, or
//! when `WEB_AUDIT_ROUTES` is unset; `--strict` promotes skips to failures.
//! This command detects the package manager from lockfile markers (reusing
//! `init::node_manifest::detect_package_manager`), checks the three peer
//! dependencies against documented minimum versions, offers to install what is
//! missing, and patches `do-harness.toml` with a `[env]` block carrying the
//! `WEB_AUDIT_*` values so `verify` stops skipping.

use anyhow::{Context, Result, bail};
use std::collections::BTreeMap;
use std::io::{BufRead, IsTerminal, Write};
use std::path::Path;

use crate::init::node_manifest::PackageManager;

/// Peer dependencies the web audit pack requires, with the minimum versions the
/// shipped runners are tested against (documented in `docs/adoption.md`).
/// An installed version below its floor is treated as missing.
pub(crate) const PEER_DEPS: [(&str, &str); 3] = [
    ("playwright", "1.40"),
    ("@axe-core/playwright", "4.8"),
    ("lighthouse", "11.0"),
];

/// `WEB_AUDIT_*` variables `setup web` writes into the `[env]` table, with the
/// documented defaults. `WEB_AUDIT_ROUTES` is the one variable without a
/// runner-side default; `"/"` is a safe placeholder the adopter replaces.
pub(crate) const WEB_ENV_DEFAULTS: [(&str, &str); 2] = [
    ("WEB_AUDIT_ROUTES", "/"),
    ("WEB_AUDIT_BASE_URL", "http://127.0.0.1:3000"),
];

/// Sensor names the web pack runs, for the post-setup `verify --only` hint.
const WEB_SENSORS: &str = "viewport-ux,a11y,console,perf,visual,i18n,web-ui-tests";

/// Runs `setup web` in `root`.
///
/// `yes` skips the interactive install prompt; `dry_run` reports without
/// installing or writing files.
pub async fn run(root: &Path, yes: bool, dry_run: bool) -> Result<()> {
    let config_path = root.join("do-harness.toml");
    let config_text = match std::fs::read_to_string(&config_path) {
        Ok(text) => text,
        Err(_) => {
            bail!(
                "{} does not exist; scaffold it first with `do-harness init --language web`",
                config_path.display()
            )
        }
    };

    let pm = crate::init::node_manifest::detect_package_manager(root);
    println!("package manager: {}", pm.as_str());

    let missing = missing_peer_deps(root);
    let mut install = false;
    let mut prompted = false;
    if missing.is_empty() {
        println!("all web audit peer dependencies are installed");
    } else {
        println!(
            "missing (or below minimum version) web audit peer dependencies: {}",
            missing.join(", ")
        );
        let (program, args_prefix) = install_command(pm);
        let install_cmd = format!(
            "{} {} {}",
            program,
            args_prefix.join(" "),
            missing.join(" ")
        );
        if yes {
            install = true;
        } else if std::io::stdin().is_terminal() {
            prompted = true;
            print!("install now? [y/N] ");
            std::io::stdout()
                .flush()
                .context("failed to flush prompt")?;
            let mut line = String::new();
            std::io::stdin()
                .lock()
                .read_line(&mut line)
                .context("failed to read install prompt")?;
            install = matches!(line.trim().to_ascii_lowercase().as_str(), "y" | "yes");
        } else {
            println!("run this to install them: {install_cmd}");
        }
        if install && !dry_run {
            let status = std::process::Command::new(program)
                .args(&args_prefix)
                .args(&missing)
                .current_dir(root)
                .status()
                .with_context(|| format!("failed to run {program}"))?;
            if !status.success() {
                bail!("{program} install failed ({status})");
            }
        }
    }

    // The config patch is independent of installation: even when dependencies
    // remain missing, the [env] block is written so the web sensors stop
    // skipping once they are installed.
    let (patched, additions) = patch_config_text(&config_text);
    if dry_run {
        if !additions.is_empty() {
            println!("would add to [env]: {}", describe_env(&additions));
        }
        println!("\nconfirm the web pack with:\n  do-harness verify --only {WEB_SENSORS}");
        return Ok(());
    }
    if patched != config_text {
        std::fs::write(&config_path, &patched)
            .with_context(|| format!("failed to write {}", config_path.display()))?;
        println!("patched {} with:", config_path.display());
        println!("{}", describe_env(&additions));
    } else {
        println!("do-harness.toml already carries the web audit environment");
    }

    if !missing.is_empty() && !install && !prompted {
        bail!(
            "web audit peer dependencies are missing; run `do-harness setup web --yes` to install them"
        );
    }
    if !missing.is_empty() && !install {
        println!(
            "note: {} are still missing; they will SKIP until installed",
            missing.join(", ")
        );
    }
    println!("\nconfirm the web pack with:\n  do-harness verify --only {WEB_SENSORS}");
    Ok(())
}

/// Peer dependencies that are missing or below their minimum version.
pub(crate) fn missing_peer_deps(root: &Path) -> Vec<&'static str> {
    PEER_DEPS
        .iter()
        .filter(|(name, floor)| {
            !installed_version(root, name)
                .is_some_and(|installed| version_at_least(&installed, floor))
        })
        .map(|(name, _)| *name)
        .collect()
}

/// Reads the installed version of `name` from `node_modules/<name>/package.json`.
pub(crate) fn installed_version(root: &Path, name: &str) -> Option<String> {
    let path = root.join("node_modules").join(name).join("package.json");
    let text = std::fs::read_to_string(path).ok()?;
    let value: serde_json::Value = serde_json::from_str(&text).ok()?;
    value.get("version")?.as_str().map(ToOwned::to_owned)
}

/// Parses the leading numeric `major.minor.patch` triple of a version string;
/// a prerelease suffix is ignored. Returns `None` for unparsable strings.
pub(crate) fn parse_version(s: &str) -> Option<(u64, u64, u64)> {
    let mut parts = s
        .split(|c: char| !c.is_ascii_digit())
        .filter(|part| !part.is_empty());
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next().and_then(|p| p.parse().ok()).unwrap_or(0);
    let patch = parts.next().and_then(|p| p.parse().ok()).unwrap_or(0);
    Some((major, minor, patch))
}

/// Whether `installed` is at least `floor` (both parsed as numeric triples).
pub(crate) fn version_at_least(installed: &str, floor: &str) -> bool {
    match (parse_version(installed), parse_version(floor)) {
        (Some(a), Some(b)) => a >= b,
        _ => false,
    }
}

/// The install command prefix for a package manager.
pub(crate) fn install_command(pm: PackageManager) -> (&'static str, Vec<&'static str>) {
    match pm {
        PackageManager::Npm => ("npm", vec!["install", "-D"]),
        PackageManager::Pnpm => ("pnpm", vec!["add", "-D"]),
        PackageManager::Yarn => ("yarn", vec!["add", "-D"]),
        PackageManager::Bun => ("bun", vec!["add", "-d"]),
    }
}

/// Patches `text` with the web audit `[env]` block.
///
/// Existing `[env]` string values are preserved; only missing keys are added.
/// Returns the new text and the keys that were newly added.
pub(crate) fn patch_config_text(text: &str) -> (String, BTreeMap<String, String>) {
    let existing = existing_env_strings(text);
    let additions: BTreeMap<String, String> = WEB_ENV_DEFAULTS
        .iter()
        .filter(|(key, _)| !existing.contains_key(*key))
        .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
        .collect();
    if additions.is_empty() {
        return (text.to_owned(), BTreeMap::new());
    }
    let mut merged = existing;
    merged.extend(additions.clone());

    let block = format!(
        "# Web pack environment (written by `do-harness setup web`).\n\
         # Replace WEB_AUDIT_ROUTES with the routes your app serves before running verify.\n{}",
        render_env_table(&merged)
    );
    match find_env_table_lines(text) {
        None => {
            let mut out = text.to_owned();
            if !out.ends_with('\n') {
                out.push('\n');
            }
            out.push('\n');
            out.push_str(&block);
            out.push('\n');
            (out, additions)
        }
        Some((start, end)) => {
            let lines: Vec<&str> = text.lines().collect();
            if !env_table_is_simple(&lines, start, end) {
                // A hand-maintained [env] table (nested tables, dotted keys,
                // non-string values) is left untouched rather than flattened.
                return (text.to_owned(), BTreeMap::new());
            }
            let mut out = lines[..start].to_vec();
            out.extend(block.lines());
            out.extend(&lines[end..]);
            let mut out = out.join("\n");
            out.push('\n');
            (out, additions)
        }
    }
}

/// Whether every line of a `[env]` table region is a comment, a blank line,
/// or a plain `key = "string"` assignment (so it can be rewritten safely).
fn env_table_is_simple(lines: &[&str], start: usize, end: usize) -> bool {
    lines[start + 1..end].iter().all(|line| {
        let trimmed = line.trim();
        trimmed.is_empty()
            || trimmed.starts_with('#')
            || trimmed
                .split_once('=')
                .is_some_and(|(key, value)| !key.contains('.') && value.trim().starts_with('"'))
    })
}

/// Existing top-level `[env]` string values in the config text.
fn existing_env_strings(text: &str) -> BTreeMap<String, String> {
    let Ok(table) = text.parse::<toml::Table>() else {
        return BTreeMap::new();
    };
    table
        .get("env")
        .and_then(toml::Value::as_table)
        .map(|env| {
            env.iter()
                .filter_map(|(key, value)| value.as_str().map(|s| (key.clone(), s.to_owned())))
                .collect()
        })
        .unwrap_or_default()
}

/// Byte-free line range of the top-level `[env]` table, when present.
fn find_env_table_lines(text: &str) -> Option<(usize, usize)> {
    let lines: Vec<&str> = text.lines().collect();
    let start = lines.iter().position(|line| line.trim_start() == "[env]")?;
    let end = lines[start + 1..]
        .iter()
        .position(|line| line.starts_with('[') && !line.starts_with(' '))
        .map(|offset| start + 1 + offset)
        .unwrap_or(lines.len());
    Some((start, end))
}

/// Renders a `[env]` table from `vars`, keys sorted.
fn render_env_table(vars: &BTreeMap<String, String>) -> String {
    let mut out = String::from("[env]\n");
    for (key, value) in vars {
        out.push_str(&format!("{key} = {value:?}\n"));
    }
    out
}

/// `k = v` lines for a key set, for display.
fn describe_env(vars: &BTreeMap<String, String>) -> String {
    vars.iter()
        .map(|(key, value)| format!("{key} = {value:?}"))
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
#[path = "web_tests.rs"]
mod tests;
