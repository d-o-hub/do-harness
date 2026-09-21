//! Deterministic digest of a managed skill tree.
//!
//! The digest is the content anchor for `skills drift` and a persisted
//! cross-repo contract: a copy of a shared skill hashes alike wherever it is
//! checked out, so the framing here changes only with a deliberate manifest
//! migration and a re-blessed golden vector.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use super::cache::sha256_hex;

/// Digest of a directory tree: `sha256` over `<path>\0<file-sha256>\n` lines.
///
/// Paths are relative to `dir` — never to the checkout that holds the copy — so
/// the same tree hashes alike in every repository. They are forward-slashed,
/// sorted by byte order, and framed so no concatenation is ambiguous; empty
/// directories do not appear. File symlinks are followed (the target's content
/// is hashed), a directory symlink *inside* the tree is an error because its
/// contents are not well-defined for a pinned tree, and other file types are
/// rejected rather than silently skipped.
///
/// The tree root is the caller's business: `skills drift` resolves the managed
/// directory and rejects one that escapes the repository before calling this,
/// so a root that is itself a symlink is fine while it stays inside the root.
///
/// # Errors
///
/// Returns an error when the tree cannot be read or contains an entry the digest
/// cannot describe (a non-UTF-8 file name, a directory symlink, a socket, or a
/// fifo).
pub fn tree_digest(dir: &Path) -> Result<String> {
    let mut files = Vec::new();
    collect_files(dir, "", &mut files)?;
    files.sort_by(|left, right| left.0.cmp(&right.0));

    let mut canonical = String::new();
    for (relative, absolute) in files {
        let bytes = fs::read(&absolute).with_context(|| format!("reading {relative}"))?;
        canonical.push_str(&relative);
        canonical.push('\0');
        canonical.push_str(&sha256_hex(&bytes));
        canonical.push('\n');
    }
    Ok(sha256_hex(canonical.as_bytes()))
}

/// Collects `(relative path, absolute path)` for every file under `dir`.
fn collect_files(dir: &Path, prefix: &str, out: &mut Vec<(String, PathBuf)>) -> Result<()> {
    let entries = fs::read_dir(dir).with_context(|| format!("reading {}", dir.display()))?;
    for entry in entries {
        let entry = entry?;
        let name = match entry.file_name().into_string() {
            Ok(name) => name,
            Err(name) => bail!(
                "managed skill tree has a non-UTF-8 file name in {}: {}; pin UTF-8 names so the digest stays unambiguous",
                dir.display(),
                name.to_string_lossy()
            ),
        };
        let relative = if prefix.is_empty() {
            name
        } else {
            format!("{prefix}/{name}")
        };
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            collect_files(&entry.path(), &relative, out)?;
        } else if file_type.is_file() {
            out.push((relative, entry.path()));
        } else if file_type.is_symlink() {
            if fs::metadata(entry.path())?.is_dir() {
                bail!(
                    "managed skill tree contains a symlinked directory: {relative}; pin real files so the digest stays well-defined"
                );
            }
            out.push((relative, entry.path()));
        } else {
            bail!("managed skill tree contains an unsupported file type: {relative}");
        }
    }
    Ok(())
}
