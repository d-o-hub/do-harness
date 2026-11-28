//! Optional attribution captured alongside a specific evidence artifact.

use std::collections::BTreeSet;
use std::io::Read;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, ensure};
use serde::Deserialize;

use super::Attempt;

const VERSION: u32 = 1;
const MAX_BYTES: u64 = 4096;
const MAX_ID_BYTES: usize = 128;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SessionBinding {
    schema_version: u32,
    pub(super) client: String,
    pub(super) session_id: String,
    pub(super) attempt_id: String,
    evidence_sha256: String,
    workspace: String,
    task_id: Option<i64>,
}

pub(super) fn read(
    evidence: &Path,
    digest: &str,
    document: &crate::evidence::EvidenceDocument,
) -> Result<Option<SessionBinding>> {
    let mut sidecar = evidence.as_os_str().to_os_string();
    sidecar.push(".session.json");
    let path = PathBuf::from(sidecar);
    let metadata = match std::fs::symlink_metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error).with_context(|| format!("read {}", path.display())),
    };
    ensure!(
        metadata.is_file(),
        "session association {} must be a regular file",
        path.display()
    );
    let mut bytes = Vec::new();
    std::fs::File::open(&path)?
        .take(MAX_BYTES + 1)
        .read_to_end(&mut bytes)?;
    ensure!(
        u64::try_from(bytes.len())? <= MAX_BYTES,
        "session association {} exceeds {MAX_BYTES} bytes",
        path.display()
    );
    let binding: SessionBinding = serde_json::from_slice(&bytes)
        .with_context(|| format!("invalid session association {}", path.display()))?;
    ensure!(
        binding.schema_version == VERSION
            && valid_id(&binding.client)
            && valid_id(&binding.session_id)
            && valid_id(&binding.attempt_id),
        "unsupported or invalid session association {}",
        path.display()
    );
    ensure!(
        binding.evidence_sha256 == digest
            && binding.workspace == document.root
            && binding.task_id == document.task_id
            && evidence
                .parent()
                .and_then(Path::file_name)
                .and_then(std::ffi::OsStr::to_str)
                == Some(&binding.attempt_id),
        "conflicting session association {} for evidence {}",
        path.display(),
        evidence.display()
    );
    Ok(Some(binding))
}

fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_ID_BYTES
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_' | b':'))
}

pub(super) fn check_attempt_ids(attempts: &[Attempt]) -> Result<()> {
    let mut ids = BTreeSet::new();
    for binding in attempts
        .iter()
        .filter_map(|attempt| attempt.session.as_ref())
    {
        ensure!(
            ids.insert(&binding.attempt_id),
            "conflicting session association for attempt '{}'",
            binding.attempt_id
        );
    }
    Ok(())
}
