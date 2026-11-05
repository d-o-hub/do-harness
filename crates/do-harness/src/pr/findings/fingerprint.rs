//! Fingerprint and digest generation for findings provenance.

use sha2::{Digest, Sha256};

use super::model::Finding;

fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

/// Computes a stable fingerprint for a finding across matrix runs and retries.
#[must_use]
pub fn compute_fingerprint(
    repo: &str,
    tool: &str,
    rule_id: &str,
    path: &str,
    line: Option<u64>,
    message: &str,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(repo.as_bytes());
    hasher.update(b":");
    hasher.update(tool.as_bytes());
    hasher.update(b":");
    hasher.update(rule_id.as_bytes());
    hasher.update(b":");
    hasher.update(path.as_bytes());
    hasher.update(b":");
    if let Some(l) = line {
        hasher.update(l.to_string().as_bytes());
    }
    hasher.update(b":");
    // Normalize signature/message by trimming whitespace
    hasher.update(message.trim().as_bytes());
    sha256_hex(&hasher.finalize())
}

/// Computes a canonical SHA-256 digest over sorted findings.
#[must_use]
pub fn compute_findings_digest(findings: &[Finding]) -> String {
    let mut sorted = findings.to_vec();
    sorted.sort_by(|a, b| a.fingerprint.cmp(&b.fingerprint));

    let mut hasher = Sha256::new();
    for f in &sorted {
        hasher.update(f.fingerprint.as_bytes());
        hasher.update(b":");
        hasher.update(f.severity.as_str().as_bytes());
        hasher.update(b":");
        hasher.update(f.classification.as_str().as_bytes());
        hasher.update(b"\n");
    }
    sha256_hex(&hasher.finalize())
}
