//! Serialize concurrent writers and replace evidence atomically.
use std::path::Path;

use anyhow::Result;
use fs2::FileExt;

use super::EvidenceDocument;

/// Chain and atomically replace a document. Readers see either complete version.
pub(crate) fn write(root: &Path, path: &Path, document: &mut EvidenceDocument) -> Result<()> {
    crate::managed_path::evidence_path(root, path)?;
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    std::fs::create_dir_all(parent)?;
    let lock_path = path.with_extension("lock");
    crate::managed_path::evidence_path(root, &lock_path)?;
    let lock = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(lock_path)?;
    lock.lock_exclusive()?;
    let previous = crate::managed_path::read_evidence(path)
        .ok()
        .and_then(|b| serde_json::from_slice::<EvidenceDocument>(&b).ok())
        .filter(|doc| doc.verify_chain(doc.prev_hash.as_deref()))
        .map(|doc| doc.chain_hash)
        .filter(|hash| !hash.is_empty());
    document.seal(previous)?;
    crate::managed_path::evidence_path(root, path)?;
    crate::managed_path::write_atomic(path, &serde_json::to_vec_pretty(document)?)?;
    Ok(())
}
