//! Tamper-evident audit log for proxy decisions — hash-chained JSONL.

#![forbid(unsafe_code)]

use std::io::Write as _;
use std::path::{Path, PathBuf};

use anyhow::Result;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{ForwardDecision, McpLikeToolCall};

/// Canonical payload for a decision: sorted JSON keys via `serde_json::Value`.
fn canonical_json(value: &serde_json::Value) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "{}".to_string())
}

/// Computes SHA-256 chain hash: `SHA-256(prev || "|" || payload)`, `prev` defaults to `GENESIS`.
fn chain_hash(prev: Option<&str>, payload: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(prev.unwrap_or("GENESIS").as_bytes());
    hasher.update(b"|");
    hasher.update(payload.as_bytes());
    hex::encode(hasher.finalize())
}

/// A single audit record persisted as JSONL.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AuditRecord {
    /// Monotonic sequence.
    pub seq: u64,
    /// Previous chain hash.
    pub prev_hash: String,
    /// Chain hash for this record.
    pub chain_hash: String,
    /// Unix millis.
    pub created_at: i64,
    /// Tool name.
    pub tool: String,
    /// Optional params.
    pub params: Option<serde_json::Value>,
    /// Decision outcome.
    pub decision: String,
    /// Human-readable reason on deny.
    pub reason: Option<String>,
}

impl AuditRecord {
    fn new(
        seq: u64,
        prev_hash: String,
        call: &McpLikeToolCall,
        decision: &ForwardDecision,
    ) -> Self {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(0));
        let (decision_str, reason) = match decision {
            ForwardDecision::Allow => ("allow".to_string(), None),
            ForwardDecision::Deny { reason } => ("deny".to_string(), Some(reason.clone())),
        };
        // Build canonical payload for hashing: decision + tool + params.
        let payload_for_hash = serde_json::json!({
            "tool": call.tool,
            "params": call.params,
            "decision": decision_str,
            "reason": reason,
        });
        let canonical = canonical_json(&payload_for_hash);
        let chain_hash = chain_hash(Some(&prev_hash), &canonical);
        // For genesis, use GENESIS as prev_hash already set; compute above from prev_hash which may be GENESIS.
        // But for audit correctness, when prev_hash is GENESIS, chain_hash is chain_hash(GENESIS, payload).
        // So we compute correctly: if prev_hash == "GENESIS", we passed "GENESIS" as prev.
        Self {
            seq,
            prev_hash,
            chain_hash,
            created_at: now,
            tool: call.tool.clone(),
            params: call.params.clone(),
            decision: decision_str,
            reason,
        }
    }
}

/// Append-only audit log writer.
#[derive(Debug)]
pub struct AuditLog {
    path: PathBuf,
    next_seq: u64,
    prev_hash: String,
}

impl AuditLog {
    /// Opens or creates the audit log at `path`, recovering `next_seq` and `prev_hash` from existing file.
    ///
    /// # Errors
    ///
    /// Returns error if file cannot be read or is corrupt.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        let (next_seq, prev_hash) = if path.exists() {
            let content = std::fs::read_to_string(&path)?;
            let mut seq = 0u64;
            let mut prev = "GENESIS".to_string();
            for line in content.lines().filter(|l| !l.trim().is_empty()) {
                let record: AuditRecord = serde_json::from_str(line)?;
                // Verify chain while loading.
                let payload_for_hash = serde_json::json!({
                    "tool": record.tool,
                    "params": record.params,
                    "decision": record.decision,
                    "reason": record.reason,
                });
                let canonical = canonical_json(&payload_for_hash);
                let expected = chain_hash(Some(&prev), &canonical);
                if expected != record.chain_hash {
                    anyhow::bail!("audit log tamper detected at seq {}", record.seq);
                }
                if record.prev_hash != prev {
                    anyhow::bail!("audit log prev_hash mismatch at seq {}", record.seq);
                }
                seq = record.seq;
                prev.clone_from(&record.chain_hash);
            }
            (seq + 1, prev)
        } else {
            if let Some(parent) = path.parent() {
                if !parent.as_os_str().is_empty() {
                    std::fs::create_dir_all(parent)?;
                }
            }
            (1, "GENESIS".to_string())
        };
        Ok(Self {
            path,
            next_seq,
            prev_hash,
        })
    }

    /// Returns current sequence that will be used next.
    #[must_use]
    pub fn next_seq(&self) -> u64 {
        self.next_seq
    }

    /// Appends a decision record.
    ///
    /// # Errors
    ///
    /// Returns error if serialization or file append fails.
    pub fn append(
        &mut self,
        call: &McpLikeToolCall,
        decision: &ForwardDecision,
    ) -> Result<AuditRecord> {
        let record = AuditRecord::new(self.next_seq, self.prev_hash.clone(), call, decision);
        let line = serde_json::to_string(&record)?;
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;
        writeln!(file, "{line}")?;
        self.next_seq += 1;
        self.prev_hash.clone_from(&record.chain_hash);
        Ok(record)
    }

    /// Verifies the entire file's hash chain.
    ///
    /// # Errors
    ///
    /// Returns error if tampered.
    pub fn verify(path: impl AsRef<Path>) -> Result<()> {
        Self::open(path)?;
        Ok(())
    }

    /// Returns path.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use serde_json::json;

    use crate::{ForwardDecision, McpLikeToolCall};

    #[test]
    fn test_audit_append_and_verify_chain() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("audit.jsonl");
        let mut log = AuditLog::open(&path).expect("open");
        assert_eq!(log.next_seq(), 1);

        let call = McpLikeToolCall::new("data.read", Some(json!({"path":"/tmp/x"})));
        let r1 = log.append(&call, &ForwardDecision::Allow).expect("append");
        assert_eq!(r1.seq, 1);
        assert_eq!(r1.prev_hash, "GENESIS");

        let call2 = McpLikeToolCall::new("shell.exec", None);
        let r2 = log
            .append(
                &call2,
                &ForwardDecision::Deny {
                    reason: "denied".to_string(),
                },
            )
            .expect("append");
        assert_eq!(r2.seq, 2);
        assert_eq!(r2.prev_hash, r1.chain_hash);

        // Verify chain loads correctly.
        AuditLog::verify(&path).expect("verify");
        let log2 = AuditLog::open(&path).expect("reopen");
        assert_eq!(log2.next_seq(), 3);
    }

    #[test]
    fn test_audit_tamper_detection() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("audit.jsonl");
        let mut log = AuditLog::open(&path).expect("open");
        let call = McpLikeToolCall::new("data.read", Some(json!({"path":"/tmp/x"})));
        log.append(&call, &ForwardDecision::Allow).expect("append");

        // Tamper file.
        let mut content = std::fs::read_to_string(&path).expect("read");
        content = content.replace("data.read", "tampered");
        std::fs::write(&path, content).expect("write");

        let err = AuditLog::open(&path).expect_err("should fail on tamper");
        assert!(err.to_string().contains("tamper"));
    }

    #[test]
    fn test_chain_hash_genesis() {
        let h1 = chain_hash(None, r#"{"a":1}"#);
        let h2 = chain_hash(Some("GENESIS"), r#"{"a":1}"#);
        assert_eq!(h1, h2);
        let h3 = chain_hash(Some(&h1), r#"{"a":1}"#);
        assert_ne!(h1, h3);
    }
}
