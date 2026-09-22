//! Every embedded file must live inside the crate.
//!
//! `cargo package` refuses to follow a path out of the package root, so an
//! `include_str!` with a `..` that escapes the crate compiles locally and fails
//! only when the tarball is verified during `cargo publish`. Measured on the
//! v0.1.2 release: `do-harness-types` and `do-harness-db` published, then
//! `do-harness` failed eight times with
//! `error: couldn't read src/../../../.config/nextest.toml`.
//!
//! Files that genuinely live outside the crate are symlinked into `assets/`
//! (`compliance.md`, `methods.json`, `nextest.toml`); `cargo package`
//! dereferences the symlink, so the embedded path stays inside the crate.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::{Component, Path, PathBuf};

/// Returns the `include_str!` / `include_bytes!` literals in `source`.
///
/// Mentions inside string literals or doc examples are skipped (the macro name
/// must not be preceded by a quote or an identifier character), and a literal
/// may not span lines — `crates/do-harness/src/split/scan.rs` contains the
/// text `"include_str!("` as a needle in its own guard.
fn embedded_literals(source: &str) -> Vec<String> {
    let mut found = Vec::new();
    for marker in ["include_str!(", "include_bytes!("] {
        let mut offset = 0;
        while let Some(relative) = source[offset..].find(marker) {
            let start = offset + relative;
            offset = start + marker.len();
            let preceded = source[..start].chars().next_back();
            if preceded.is_some_and(|c| c == '"' || c == '_' || c.is_alphanumeric()) {
                continue;
            }
            let after = &source[offset..];
            let Some(rest) = after.strip_prefix('"') else {
                continue;
            };
            let Some(end) = rest.find('"') else { continue };
            if rest[..end].contains('\n') {
                continue;
            }
            found.push(rest[..end].to_owned());
        }
    }
    found
}

/// Resolves `path` lexically, without touching the filesystem.
fn normalize(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::ParentDir => {
                normalized.pop();
            }
            other => normalized.push(other.as_os_str()),
        }
    }
    normalized
}

fn sources(dir: &Path, files: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            sources(&path, files);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            files.push(path);
        }
    }
}

#[test]
fn embedded_files_stay_inside_the_crate() {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut files = Vec::new();
    sources(&manifest.join("src"), &mut files);
    assert!(!files.is_empty(), "no sources found to check");

    let mut checked = 0usize;
    let mut escaping = Vec::new();
    for file in &files {
        let source = std::fs::read_to_string(file).unwrap();
        let dir = file.parent().unwrap();
        for literal in embedded_literals(&source) {
            checked += 1;
            let resolved = normalize(&dir.join(&literal));
            if !resolved.starts_with(manifest) || !resolved.exists() {
                escaping.push(format!("{}: {literal}", file.display()));
            }
        }
    }

    assert!(checked > 0, "no embedded files found to check");
    assert!(
        escaping.is_empty(),
        "embedded paths must stay inside the crate (symlink external files into assets/): {escaping:?}"
    );
}
