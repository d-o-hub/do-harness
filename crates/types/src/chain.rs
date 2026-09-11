//! Canonical JSON payloads and SHA-256 hash chains shared by persisted logs.
//!
//! Both the workflow event log (`workflow_events`) and the guardian-proxy
//! audit log (`guardian-audit.jsonl`) hash a canonical payload with a
//! previous-hash prefix. Keeping one implementation here guarantees the two
//! chains cannot drift apart.

use sha2::{Digest, Sha256};

/// Canonicalizes a JSON string by parsing into `serde_json::Value` and
/// re-serializing it, which emits object keys in deterministic sorted order.
///
/// # Errors
///
/// Returns an error when `payload_json` is not valid JSON or re-serialization
/// fails.
pub fn canonical_payload(payload_json: &str) -> Result<String, serde_json::Error> {
    let value: serde_json::Value = serde_json::from_str(payload_json)?;
    serde_json::to_string(&value)
}

/// Canonicalizes a JSON value; object keys are emitted in sorted order.
///
/// # Errors
///
/// Returns an error when serialization fails.
pub fn canonical_value(value: &serde_json::Value) -> Result<String, serde_json::Error> {
    serde_json::to_string(value)
}

/// Computes the SHA-256 chain hash `SHA-256(prev || "|" || payload_json)`.
///
/// When `prev` is [`None`] the chain starts from the literal `GENESIS`
/// sentinel.
#[must_use]
pub fn chain_hash(prev: Option<&str>, payload_json: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(prev.unwrap_or("GENESIS").as_bytes());
    hasher.update(b"|");
    hasher.update(payload_json.as_bytes());
    hex::encode(hasher.finalize())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    #[test]
    fn canonical_payload_sorts_reordered_keys() {
        let first = canonical_payload(r#"{"b":2,"a":1}"#).unwrap();
        let second = canonical_payload(r#"{"a":1,"b":2}"#).unwrap();
        assert_eq!(first, r#"{"a":1,"b":2}"#);
        assert_eq!(first, second);
    }

    #[test]
    fn canonical_payload_rejects_invalid_json() {
        assert!(canonical_payload("not json").is_err());
    }

    #[test]
    fn chain_hash_is_deterministic_and_genesis_aware() {
        let payload = r#"{"a":1}"#;
        let genesis = chain_hash(None, payload);
        assert_eq!(genesis, chain_hash(Some("GENESIS"), payload));
        assert_ne!(genesis, chain_hash(Some("other"), payload));
        assert_eq!(genesis.len(), 64);
    }
}
