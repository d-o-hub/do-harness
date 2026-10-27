//! Stable-worktree containment checks for harness-managed writes.
use std::path::{Component, Path, PathBuf};

use anyhow::{Result, bail};

const MAX_EVIDENCE_BYTES: u64 = 64 * 1024 * 1024;

/// Reject traversal, symlink components and non-UTF-8 collisions before writing.
/// This is a filesystem precondition, not isolation from hostile concurrent renames.
pub(crate) fn within(root: &Path, path: &Path) -> Result<PathBuf> {
    let canonical_root = root.canonicalize()?;
    if canonical_root.to_str().is_none() || path.to_str().is_none() {
        bail!("managed paths must have an unambiguous UTF-8 spelling");
    }
    let relative = if path.is_absolute() {
        path.strip_prefix(root)
            .or_else(|_| path.strip_prefix(&canonical_root))?
    } else {
        path
    };
    if !relative
        .components()
        .all(|c| matches!(c, Component::Normal(_)))
    {
        bail!("managed paths must be relative without traversal components");
    }
    let mut current = canonical_root.clone();
    for component in relative.components() {
        current.push(component);
        match std::fs::symlink_metadata(&current) {
            Ok(metadata) => {
                if metadata.file_type().is_symlink()
                    || !current.canonicalize()?.starts_with(&canonical_root)
                {
                    bail!("managed path crosses a symlink or workspace boundary");
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
    }
    Ok(current)
}

/// Default/relative destinations belong to root. Explicit external paths remain
/// supported; they select their own directory, but cannot name a symlink leaf.
pub(crate) fn evidence_path(root: &Path, path: &Path) -> Result<PathBuf> {
    if path.to_str().is_none() {
        bail!("evidence paths must be UTF-8");
    }
    if std::fs::symlink_metadata(path).is_ok_and(|m| !m.is_file()) {
        bail!("evidence destination must be a regular file without symlinks");
    }
    if !path.is_absolute() || path.starts_with(root) {
        return within(root, path);
    }
    Ok(path.to_path_buf())
}

/// Evidence is a bounded regular file, never a pipe, device or symlink.
pub(crate) fn read_evidence(path: &Path) -> Result<Vec<u8>> {
    use std::io::Read;
    let metadata = std::fs::symlink_metadata(path)?;
    if !metadata.is_file() || metadata.len() > MAX_EVIDENCE_BYTES {
        bail!("evidence must be a regular file no larger than 64 MiB");
    }
    let file = std::fs::File::open(path)?;
    if !file.metadata()?.is_file() {
        bail!("evidence type changed during read");
    }
    let mut bytes = Vec::new();
    file.take(MAX_EVIDENCE_BYTES + 1).read_to_end(&mut bytes)?;
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > MAX_EVIDENCE_BYTES {
        bail!("evidence grew beyond the read budget");
    }
    Ok(bytes)
}

/// Atomic leaf replacement never follows an existing destination symlink.
pub(crate) fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    use std::io::Write;
    let parent = path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("missing output directory"))?;
    std::fs::create_dir_all(parent)?;
    let mut temp = tempfile::NamedTempFile::new_in(parent)?;
    temp.write_all(bytes)?;
    temp.as_file().sync_all()?;
    temp.persist(path)?;
    Ok(())
}
