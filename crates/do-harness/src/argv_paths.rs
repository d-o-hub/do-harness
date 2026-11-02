//! Path-looking sensor `argv` arguments and their existence.
//!
//! A sensor whose `argv` names a repository file that no longer exists can
//! still exit 0 — an interpreter that ignores a missing script, a wrapper that
//! swallows the error — so the gate goes green over an unmeasured tree. This
//! module resolves the arguments that look like paths and reports the ones
//! that do not exist, so `verify` can refuse the pass and `doctor` can name
//! the broken gate.

use std::path::Path;

use crate::config::SensorSpec;

/// File extensions that make a bare token path-looking even without a
/// separator (`Cargo.toml`, `foo.rs`).
const PATH_EXTENSIONS: &[&str] = &[
    "sh", "bash", "zsh", "fish", "ps1", "bat", "cmd", "py", "rb", "pl", "js", "mjs", "cjs", "ts",
    "tsx", "jsx", "rs", "go", "java", "kt", "c", "h", "cc", "cpp", "hpp", "toml", "json", "jsonc",
    "yaml", "yml", "md", "txt", "lock", "cfg", "ini", "xml", "html", "css", "scss", "sql", "proto",
    "gradle",
];

/// Characters that make a token a shell expression rather than a path.
const SHELL_META: &[char] = &[
    ';', '|', '&', '$', '`', '"', '\'', '(', ')', '<', '>', '*', '?', '[', ']', '{', '}', '~', '!',
    '#',
];

/// A path-looking `argv` argument that does not exist under the workspace
/// root.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MissingArgvPath {
    /// Sensor name as configured.
    pub sensor: String,
    /// The argument exactly as written in `argv`.
    pub arg: String,
}

/// Returns the path-looking arguments in `spec.argv` that do not exist under
/// `root`, in argv order.
///
/// `argv[0]` is the program and is skipped: a missing executable already
/// fails loudly at spawn. A token is path-looking when it is a single
/// positional-looking word (no whitespace, shell metacharacters, URL or
/// `--flag=value` form) that either carries a known file extension, starts
/// with a dot, is absolute, or has a first path component that exists under
/// `root`. The first-component rule keeps refs such as `origin/main` from
/// being mistaken for paths while still catching `scripts/check-loc.sh` and
/// `scripts/does-not-exist.sh`.
#[must_use]
pub(crate) fn missing_paths(root: &Path, spec: &SensorSpec) -> Vec<String> {
    spec.argv
        .iter()
        .skip(1)
        .filter(|arg| is_path_looking(root, arg))
        .filter(|arg| !resolves(root, arg))
        .cloned()
        .collect()
}

/// Returns the missing path-looking arguments for every sensor in `specs`.
#[must_use]
pub(crate) fn missing_paths_for(root: &Path, specs: &[SensorSpec]) -> Vec<MissingArgvPath> {
    specs
        .iter()
        .flat_map(|spec| {
            missing_paths(root, spec)
                .into_iter()
                .map(|arg| MissingArgvPath {
                    sensor: spec.name.clone(),
                    arg,
                })
        })
        .collect()
}

/// Whether `arg` looks like a filesystem path rather than a flag, a value, or
/// a shell expression.
fn is_path_looking(root: &Path, arg: &str) -> bool {
    if arg.is_empty() || arg.starts_with('-') {
        return false;
    }
    if arg.contains("://") || arg.contains('@') || arg.contains('=') {
        return false;
    }
    if arg
        .chars()
        .any(|c| c.is_whitespace() || SHELL_META.contains(&c))
    {
        return false;
    }
    if has_known_extension(arg) || is_dotfile(arg) {
        return true;
    }
    let path = Path::new(arg);
    if path.is_absolute() {
        return true;
    }
    let first = arg.split(['/', '\\']).next().unwrap_or(arg);
    !first.is_empty() && root.join(first).is_dir()
}

/// Whether `arg` carries a known source, script, or configuration extension.
fn has_known_extension(arg: &str) -> bool {
    Path::new(arg)
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| {
            PATH_EXTENSIONS
                .iter()
                .any(|known| ext.eq_ignore_ascii_case(known))
        })
}

/// Whether `arg` is a dotfile such as `.gitignore` (which has no extension).
fn is_dotfile(arg: &str) -> bool {
    arg.starts_with('.') && arg != "." && arg != ".." && !arg.contains(['/', '\\'])
}

/// Whether `arg` resolves to an existing path under `root` (or as an absolute
/// path).
fn resolves(root: &Path, arg: &str) -> bool {
    let path = Path::new(arg);
    let full = if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    };
    full.exists()
}

#[cfg(test)]
#[path = "argv_paths_tests.rs"]
mod tests;
