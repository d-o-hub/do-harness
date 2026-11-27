//! Frozen review-packet assembly: canonical JSON, identity hash, git pinning.
//!
//! A packet is the sole input to a fresh-context reviewer: the objective,
//! intended behavior, explicit constraints, proposed contract, open
//! questions, and relevant paths, all pinned to a git revision. The packet
//! identity is a SHA-256 over the canonical JSON of every field except the
//! hash itself, so it is stable for identical inputs and changes when any
//! input changes. All listed paths resolve to git blob revisions at the
//! pinned commit; a missing path at that revision is a hard error, never a
//! silently dropped input.

use std::path::Path;

use anyhow::{Context, Result, bail};
use serde_json::{Map, Value, json};

/// Packet schema version.
pub const SCHEMA_VERSION: u64 = 1;
/// Default reviewer deadline (seconds).
pub const DEFAULT_TIMEOUT_S: u64 = 300;
/// Default material-findings cap.
pub const DEFAULT_MAX_FINDINGS: usize = 5;
/// Default reviewer output cap (bytes).
pub const DEFAULT_MAX_OUTPUT_BYTES: usize = 65536;

/// Fixed statement telling the reviewer the contract and rationale live in
/// the frozen design document and no author conclusion is supplied.
pub const REVIEWER_NOTE: &str = "The proposed contract and decision rationale live in the frozen design document itself. No author deliberation transcript, chat log, or summary telling the reviewer which conclusion to reach is included in this packet.";

/// Deterministic JSON: sorted keys, compact separators, UTF-8 — byte-for-byte
/// stable serialization for identity hashes and record comparisons.
///
/// # Errors
///
/// Never in practice: serializing an in-memory [`Value`] has no IO and the
/// workspace does not enable `preserve_order`, so object keys sort; the
/// fallible signature keeps serialization panic-free like the rest of the
/// crate.
pub fn canonical_json(value: &Value) -> Result<String> {
    serde_json::to_string(value).context("failed to canonicalize a JSON value")
}

/// `sha256:<hex>` over the raw string.
pub fn sha256_hex(raw: &str) -> String {
    use sha2::{Digest, Sha256};
    format!("sha256:{:x}", Sha256::digest(raw.as_bytes()))
}

fn git_output(root: &Path, args: &[&str]) -> Result<String> {
    let output = crate::changes::git_command(root)
        .args(args)
        .output()
        .with_context(|| format!("failed to run git {}", args.join(" ")))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let reason: String = stderr.trim().chars().take(300).collect();
        bail!("git {} failed: {reason}", args.join(" "));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

/// Resolves a revision to its full commit SHA.
pub fn commit_sha(root: &Path, revision: &str) -> Result<String> {
    let spec = format!("{revision}^{{commit}}");
    git_output(root, &["rev-parse", &spec])
}

/// Resolves a repository path to its git blob SHA at a revision.
pub fn blob_sha(root: &Path, revision: &str, path: &str) -> Result<String> {
    let spec = format!("{revision}:{path}");
    git_output(root, &["rev-parse", &spec])
}

/// Inputs to [`build_packet`].
#[derive(Debug, Clone)]
pub struct PacketSpec {
    /// Opaque review identifier recorded in the packet and the record.
    pub review_id: String,
    /// The concrete design risk in one or two sentences.
    pub objective: String,
    /// Expected observable behavior.
    pub intended_behavior: String,
    /// Explicit constraints.
    pub constraints: Vec<String>,
    /// The candidate contract.
    pub proposed_contract: String,
    /// Questions the author has genuinely not decided.
    pub open_questions: Vec<String>,
    /// Repository-relative source paths.
    pub source_paths: Vec<String>,
    /// Repository-relative test paths.
    pub test_paths: Vec<String>,
    /// Repository-relative document paths.
    pub doc_paths: Vec<String>,
    /// The frozen design document (repository-relative).
    pub design_doc: String,
    /// Revision every path is pinned to.
    pub revision: String,
    /// Reviewer deadline (seconds).
    pub timeout_s: u64,
    /// Material-findings cap.
    pub max_findings: usize,
    /// Reviewer output cap (bytes).
    pub max_output_bytes: usize,
}

/// Freezes a review packet against a git revision.
///
/// # Errors
///
/// Fails when the objective or contract is empty, a limit is not positive,
/// the revision is unresolvable, or any listed path does not exist at that
/// revision (revision pinning fails closed).
pub fn build_packet(root: &Path, spec: &PacketSpec) -> Result<Value> {
    if spec.objective.trim().is_empty() {
        bail!("objective must be non-empty");
    }
    if spec.proposed_contract.trim().is_empty() {
        bail!("proposed_contract must be non-empty");
    }
    if spec.timeout_s == 0 || spec.max_findings == 0 || spec.max_output_bytes == 0 {
        bail!("limits must be positive");
    }

    let commit = commit_sha(root, &spec.revision)?;
    let design_blob = blob_sha(root, &spec.revision, &spec.design_doc).with_context(|| {
        format!(
            "design document {} is not resolvable at {}",
            spec.design_doc, spec.revision
        )
    })?;
    let mut files = Map::new();
    let all_paths = spec
        .source_paths
        .iter()
        .chain(&spec.test_paths)
        .chain(&spec.doc_paths);
    for path in all_paths {
        let blob = blob_sha(root, &spec.revision, path)
            .with_context(|| format!("pinned path {path} does not exist at {}", spec.revision))?;
        files.insert(path.clone(), Value::String(blob));
    }

    Ok(json!({
        "schema_version": SCHEMA_VERSION,
        "review_id": spec.review_id,
        "objective": spec.objective,
        "intended_behavior": spec.intended_behavior,
        "constraints": spec.constraints,
        "proposed_contract": spec.proposed_contract,
        "open_questions": spec.open_questions,
        "paths": {
            "source": spec.source_paths,
            "tests": spec.test_paths,
            "docs": spec.doc_paths,
        },
        "revisions": {
            "commit": commit,
            "design_doc": {"path": spec.design_doc, "blob": design_blob},
            "files": files,
        },
        "limits": {
            "timeout_s": spec.timeout_s,
            "max_findings": spec.max_findings,
            "max_output_bytes": spec.max_output_bytes,
        },
        "reviewer_note": REVIEWER_NOTE,
    }))
}

/// Stable identity for a packet: SHA-256 over the canonical JSON of every
/// field except `packet_hash` itself.
///
/// # Errors
///
/// Propagates a canonicalization failure; see [`canonical_json`].
pub fn packet_hash(packet: &Value) -> Result<String> {
    let mut body = packet.clone();
    if let Some(map) = body.as_object_mut() {
        map.remove("packet_hash");
    }
    Ok(sha256_hex(&canonical_json(&body)?))
}
