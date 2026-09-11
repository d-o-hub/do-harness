//! Tamper-evident audit log for proxy decisions — hash-chained JSONL.

#![forbid(unsafe_code)]

use std::io::Write as _;
use std::path::{Path, PathBuf};

use crate::error::{GuardianError, Result};
use do_harness_types::{canonical_value, chain_hash};
use serde::{Deserialize, Serialize};

use crate::{ForwardDecision, McpLikeToolCall};

/// A single audit record persisted as JSONL.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
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
    ) -> Result<Self> {
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
        let canonical = canonical_value(&payload_for_hash)?;
        let hash = chain_hash(Some(&prev_hash), &canonical);
        Ok(Self {
            seq,
            prev_hash,
            chain_hash: hash,
            created_at: now,
            tool: call.tool.clone(),
            params: call.params.clone(),
            decision: decision_str,
            reason,
        })
    }
}

/// Reads the tail record's next `(seq, prev_hash)` from the log, or genesis.
///
/// Called while holding the file lock so the sequence reflects every writer's
/// committed appends, not this handle's stale in-memory state.
fn tail_of(path: &Path) -> Result<(u64, String)> {
    let content = std::fs::read_to_string(path).map_err(|io| GuardianError::AuditIo {
        path: path.display().to_string(),
        io,
    })?;
    match content.lines().rev().find(|line| !line.trim().is_empty()) {
        Some(line) => {
            let record: AuditRecord = serde_json::from_str(line)?;
            Ok((record.seq + 1, record.chain_hash))
        }
        None => Ok((1, "GENESIS".to_string())),
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
            let content = std::fs::read_to_string(&path).map_err(|io| GuardianError::AuditIo {
                path: path.display().to_string(),
                io,
            })?;
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
                let canonical = canonical_value(&payload_for_hash)?;
                let expected = chain_hash(Some(&prev), &canonical);
                let seq_i64 = i64::try_from(record.seq).unwrap_or(i64::MAX);
                if expected != record.chain_hash || record.prev_hash != prev {
                    return Err(GuardianError::Tampered { seq: seq_i64 });
                }
                seq = record.seq;
                prev.clone_from(&record.chain_hash);
            }
            (seq + 1, prev)
        } else {
            if let Some(parent) = path.parent() {
                if !parent.as_os_str().is_empty() {
                    std::fs::create_dir_all(parent).map_err(|io| GuardianError::AuditIo {
                        path: parent.display().to_string(),
                        io,
                    })?;
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
    /// The file is locked exclusively across processes and re-read under the
    /// lock, so concurrent proxy instances cannot fork `seq`/`prev_hash`; the
    /// write is `fsync`ed before the lock is released.
    ///
    /// # Errors
    ///
    /// Returns error if locking, serialization, or file append fails.
    pub fn append(
        &mut self,
        call: &McpLikeToolCall,
        decision: &ForwardDecision,
    ) -> Result<AuditRecord> {
        use fs2::FileExt as _;

        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .map_err(|io| GuardianError::AuditIo {
                path: self.path.display().to_string(),
                io,
            })?;
        file.lock_exclusive().map_err(|io| GuardianError::AuditIo {
            path: self.path.display().to_string(),
            io,
        })?;

        let (seq, prev_hash) = match tail_of(&self.path) {
            Ok(tail) => tail,
            Err(err) => {
                let _ = fs2::FileExt::unlock(&file);
                return Err(err);
            }
        };
        let record = AuditRecord::new(seq, prev_hash, call, decision)?;
        let line = serde_json::to_string(&record)?;
        let write_result = writeln!(file, "{line}")
            .and_then(|()| file.sync_all())
            .map_err(|io| GuardianError::AuditIo {
                path: self.path.display().to_string(),
                io,
            });
        // Unlock on every path so a failure cannot wedge later writers.
        let _ = fs2::FileExt::unlock(&file);
        write_result?;

        self.next_seq = seq + 1;
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

    /// Audit records are a stability contract: unknown fields are rejected.
    #[test]
    fn test_audit_record_rejects_unknown_fields() {
        let record = r#"{"seq":1,"prev_hash":"GENESIS","chain_hash":"x",
             "created_at":0,"tool":"t","params":null,"decision":"allow",
             "reason":null,"bogus":true}"#;
        assert!(serde_json::from_str::<AuditRecord>(record).is_err());
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

    /// Two writers (simulating two proxy processes) share one chain: the
    /// second append re-reads the tail under the lock instead of forking seq.
    #[test]
    fn test_two_writers_share_chain() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("audit.jsonl");
        let mut first = AuditLog::open(&path).expect("first open");
        let mut second = AuditLog::open(&path).expect("second open");
        let call = McpLikeToolCall::new("data.read", None);

        let r1 = first
            .append(&call, &ForwardDecision::Allow)
            .expect("append");
        let r2 = second
            .append(&call, &ForwardDecision::Allow)
            .expect("append");
        assert_eq!(r1.seq, 1);
        assert_eq!(r2.seq, 2);
        assert_eq!(r2.prev_hash, r1.chain_hash);

        AuditLog::verify(&path).expect("chain intact");
        let reopened = AuditLog::open(&path).expect("reopen");
        assert_eq!(reopened.next_seq(), 3);
    }
}
