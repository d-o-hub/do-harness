//! Serialize concurrent writers and replace evidence atomically.
use std::io::Write;
use std::path::Path;

use anyhow::Result;
use fs2::FileExt;

use super::EvidenceDocument;

/// Chain and atomically replace a document. Readers see either complete version.
pub(crate) fn write(path: &Path, document: &mut EvidenceDocument) -> Result<()> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    std::fs::create_dir_all(parent)?;
    let lock = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(path.with_extension("lock"))?;
    lock.lock_exclusive()?;
    let previous = std::fs::read(path)
        .ok()
        .and_then(|b| serde_json::from_slice::<EvidenceDocument>(&b).ok())
        .filter(|doc| doc.verify_chain(doc.prev_hash.as_deref()))
        .map(|doc| doc.chain_hash)
        .filter(|hash| !hash.is_empty());
    document.seal(previous)?;
    let mut temp = tempfile::NamedTempFile::new_in(parent)?;
    temp.write_all(&serde_json::to_vec_pretty(document)?)?;
    temp.as_file().sync_all()?;
    temp.persist(path)?;
    Ok(())
}
