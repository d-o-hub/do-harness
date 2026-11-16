//! Receipt construction, validation, and application.

use std::collections::BTreeMap;

use sha2::{Digest, Sha256};

use super::authorization::SnapshotInputs;
use super::model::{CheckSummary, ConversationSummary, FindingsSummary, ReadinessReport};
use super::reason::{
    Authorization, AuthorizationDecision, AuthorizationReason, MAX_RECEIPT_LIFETIME_SECONDS,
    MergeReceipt, RECEIPT_LIFETIME_SECONDS, RECEIPT_SCHEMA_VERSION, codes,
};

fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn hash_field(hasher: &mut Sha256, name: &str, value: &str) {
    hasher.update(name.as_bytes());
    hasher.update(b"=");
    hasher.update(value.as_bytes());
    hasher.update(b"\n");
}

/// Digest of the classified check set, stable across JSON field ordering.
#[must_use]
pub fn checks_digest(checks: &CheckSummary) -> String {
    let mut hasher = Sha256::new();
    hash_field(&mut hasher, "head", &checks.head_sha);
    hash_field(&mut hasher, "verdict", &checks.verdict);
    for entry in &checks.entries {
        hash_field(
            &mut hasher,
            "entry",
            &format!(
                "{}|{}|{}|{}|{}",
                entry.name,
                entry.source,
                entry.state,
                entry.conclusion.as_deref().unwrap_or("-"),
                entry.verdict.as_str()
            ),
        );
    }
    sha256_hex(&hasher.finalize())
}

/// Digest of the conversation inventory plus its triage residue.
#[must_use]
pub fn conversations_digest(conversations: &ConversationSummary) -> String {
    let mut hasher = Sha256::new();
    hash_field(
        &mut hasher,
        "inventory",
        &conversations.conversation_inventory_digest,
    );
    hash_field(
        &mut hasher,
        "review_decision",
        conversations.review_decision.as_deref().unwrap_or("-"),
    );
    for item in &conversations.needs_triage {
        hash_field(&mut hasher, "triage", &item.item_ref);
    }
    sha256_hex(&hasher.finalize())
}

/// Digest of the classified findings.
#[must_use]
pub fn findings_digest(findings: &FindingsSummary) -> String {
    let mut hasher = Sha256::new();
    hash_field(&mut hasher, "digest", &findings.digest);
    hash_field(
        &mut hasher,
        "base_verified",
        &findings.base_verified.to_string(),
    );
    sha256_hex(&hasher.finalize())
}

/// Lifetime a receipt may carry for a collection that took `generation` seconds.
///
/// The floor is the documented 60-second contract; the ceiling keeps a
/// pathological API call from widening the reuse window without bound. A
/// generation of zero still yields the floor.
fn receipt_lifetime_seconds(generation: i64) -> i64 {
    generation
        .max(0)
        .saturating_mul(2)
        .clamp(RECEIPT_LIFETIME_SECONDS, MAX_RECEIPT_LIFETIME_SECONDS)
}

#[allow(clippy::too_many_lines)] // one assignment per receipt field, in wire order
pub(super) fn build_receipt(inputs: &SnapshotInputs<'_>) -> MergeReceipt {
    let mut labels: Vec<String> = inputs
        .view
        .labels
        .iter()
        .map(|label| label.name.clone())
        .collect();
    labels.sort();

    let checks_sha256 = checks_digest(inputs.checks);
    let conversations_sha256 = conversations_digest(inputs.conversations);
    let findings_sha256 = findings_digest(inputs.findings);
    let policy_revision = inputs
        .policy_revision
        .clone()
        .unwrap_or_else(|| "absent".to_owned());
    let (ancestry, ancestry_base, ancestry_merge_base) = match inputs.ancestry {
        Some(proof) => (
            proof.decision.as_str().to_owned(),
            proof.base_commit_sha.clone(),
            proof.merge_base_sha.clone(),
        ),
        None => ("unknown".to_owned(), String::new(), String::new()),
    };

    let mut hasher = Sha256::new();
    let pairs: BTreeMap<&str, String> = BTreeMap::from([
        ("pr", inputs.pr.to_string()),
        ("base_ref_name", inputs.view.base_ref_name.clone()),
        ("base_ref_oid", inputs.view.base_ref_oid.clone()),
        ("head_ref_name", inputs.view.head_ref_name.clone()),
        ("head_ref_oid", inputs.view.head_ref_oid.clone()),
        (
            "head_repository",
            inputs
                .view
                .head_repository
                .as_ref()
                .map_or_else(|| "-".to_owned(), |repo| repo.name_with_owner.clone()),
        ),
        ("state", inputs.view.state.clone()),
        ("is_draft", inputs.view.is_draft.to_string()),
        ("labels", labels.join(",")),
        ("mergeable", inputs.merge_state.mergeable.clone()),
        ("merge_state_status", inputs.merge_state.status.clone()),
        ("auto_merge", inputs.merge_state.auto_merge.to_string()),
        (
            "review_decision",
            inputs
                .view
                .review_decision
                .clone()
                .unwrap_or_else(|| "-".to_owned()),
        ),
        ("checks", checks_sha256.clone()),
        ("conversations", conversations_sha256.clone()),
        ("findings", findings_sha256.clone()),
        ("policy", policy_revision.clone()),
        (
            "policy_source",
            match inputs.policy_source {
                crate::pr::policy_source::PolicySource::Overlay => "overlay".to_owned(),
                crate::pr::policy_source::PolicySource::Derived => "derived".to_owned(),
            },
        ),
        ("ancestry", ancestry.clone()),
        ("ancestry_base", ancestry_base.clone()),
        ("ancestry_merge_base", ancestry_merge_base.clone()),
    ]);
    for (name, value) in &pairs {
        hash_field(&mut hasher, name, value);
    }
    let snapshot_sha256 = sha256_hex(&hasher.finalize());

    let generation_seconds = inputs.generation_seconds.max(0);
    let lifetime = receipt_lifetime_seconds(generation_seconds);

    MergeReceipt {
        schema_version: RECEIPT_SCHEMA_VERSION,
        pr: inputs.pr,
        issued_at_unix: inputs.now_unix,
        expires_at_unix: inputs.now_unix.saturating_add(lifetime),
        generation_seconds,
        snapshot_sha256,
        base_ref_name: inputs.view.base_ref_name.clone(),
        base_ref_oid: inputs.view.base_ref_oid.clone(),
        head_ref_name: inputs.view.head_ref_name.clone(),
        head_ref_oid: inputs.view.head_ref_oid.clone(),
        head_repository: inputs
            .view
            .head_repository
            .as_ref()
            .map(|repo| repo.name_with_owner.clone()),
        state: inputs.view.state.clone(),
        is_draft: inputs.view.is_draft,
        labels,
        mergeable: inputs.merge_state.mergeable.clone(),
        merge_state_status: inputs.merge_state.status.clone(),
        auto_merge: inputs.merge_state.auto_merge,
        review_decision: inputs.view.review_decision.clone(),
        checks_sha256,
        conversations_sha256,
        findings_sha256,
        policy_revision,
        policy_source: match inputs.policy_source {
            crate::pr::policy_source::PolicySource::Overlay => "overlay",
            crate::pr::policy_source::PolicySource::Derived => "derived",
        }
        .to_owned(),
        ancestry,
        ancestry_base_commit: ancestry_base,
        ancestry_merge_base,
    }
}

/// Why a supplied receipt does not authorize the current snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReceiptVerdict {
    /// The receipt matches the recomputed snapshot and is still fresh.
    Fresh,
    /// The receipt expired.
    Expired,
    /// The snapshot changed since the receipt was issued.
    Changed,
    /// The receipt is malformed or belongs to another PR.
    Invalid(String),
}

/// Validates a previously issued receipt against a freshly computed one.
#[must_use]
pub fn validate_receipt(
    supplied: &MergeReceipt,
    refreshed: &MergeReceipt,
    now_unix: i64,
) -> ReceiptVerdict {
    if supplied.schema_version != RECEIPT_SCHEMA_VERSION {
        return ReceiptVerdict::Invalid(format!(
            "receipt schema_version is {} (expected {RECEIPT_SCHEMA_VERSION})",
            supplied.schema_version
        ));
    }
    if supplied.pr != refreshed.pr {
        return ReceiptVerdict::Invalid(format!(
            "receipt is for PR {} but the requested PR is {}",
            supplied.pr, refreshed.pr
        ));
    }
    let lifetime = supplied
        .expires_at_unix
        .saturating_sub(supplied.issued_at_unix);
    if !(RECEIPT_LIFETIME_SECONDS..=MAX_RECEIPT_LIFETIME_SECONDS).contains(&lifetime) {
        return ReceiptVerdict::Invalid(format!(
            "receipt lifetime is {lifetime}s (expected {RECEIPT_LIFETIME_SECONDS}..={MAX_RECEIPT_LIFETIME_SECONDS} seconds)"
        ));
    }
    if supplied.issued_at_unix > now_unix.saturating_add(RECEIPT_LIFETIME_SECONDS) {
        return ReceiptVerdict::Invalid("receipt was issued in the future".to_owned());
    }
    if now_unix > supplied.expires_at_unix {
        return ReceiptVerdict::Expired;
    }
    if supplied.snapshot_sha256 != refreshed.snapshot_sha256 {
        return ReceiptVerdict::Changed;
    }
    ReceiptVerdict::Fresh
}

/// Authorization for a snapshot that never stabilized.
#[must_use]
pub fn unstable(message: String) -> Authorization {
    Authorization {
        schema_version: RECEIPT_SCHEMA_VERSION,
        decision: AuthorizationDecision::Unknown,
        reasons: vec![AuthorizationReason::new(codes::SNAPSHOT_MOVED, message)],
        receipt: None,
    }
}

/// Current UTC epoch seconds.
#[must_use]
pub fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |delta| {
            i64::try_from(delta.as_secs()).unwrap_or(i64::MAX)
        })
}

/// Applies a caller-supplied receipt to a freshly computed authorization.
///
/// The refreshed authorization is never upgraded: a receipt can only turn
/// `MERGE_ALLOWED` into `UNKNOWN` (expired, changed, or malformed), never the
/// other way round.
#[must_use]
pub fn apply_expected_receipt(
    refreshed: Authorization,
    document: &str,
    now_unix: i64,
) -> Authorization {
    let supplied = match extract_receipt(document) {
        Ok(receipt) => receipt,
        Err(message) => {
            return Authorization {
                schema_version: RECEIPT_SCHEMA_VERSION,
                decision: AuthorizationDecision::Unknown,
                reasons: vec![AuthorizationReason::new(codes::RECEIPT_INVALID, message)],
                receipt: None,
            };
        }
    };

    // A refresh that is not allowed already carries its own reasons; adding a
    // receipt verdict would bury the real cause.
    let Some(refreshed_receipt) = refreshed.receipt.clone() else {
        return refreshed;
    };

    match validate_receipt(&supplied, &refreshed_receipt, now_unix) {
        ReceiptVerdict::Fresh => refreshed,
        ReceiptVerdict::Expired => Authorization {
            schema_version: RECEIPT_SCHEMA_VERSION,
            decision: AuthorizationDecision::Unknown,
            reasons: vec![AuthorizationReason::new(
                codes::RECEIPT_EXPIRED,
                format!(
                    "receipt expired at unix {expires} (now {now_unix}); issued at {issued} after {generation}s of collection",
                    expires = supplied.expires_at_unix,
                    issued = supplied.issued_at_unix,
                    generation = supplied.generation_seconds,
                ),
            )],
            receipt: None,
        },
        ReceiptVerdict::Changed => Authorization {
            schema_version: RECEIPT_SCHEMA_VERSION,
            decision: AuthorizationDecision::Unknown,
            reasons: vec![AuthorizationReason::new(
                codes::RECEIPT_CHANGED,
                "the snapshot changed since the receipt was issued",
            )],
            receipt: None,
        },
        ReceiptVerdict::Invalid(message) => Authorization {
            schema_version: RECEIPT_SCHEMA_VERSION,
            decision: AuthorizationDecision::Unknown,
            reasons: vec![AuthorizationReason::new(codes::RECEIPT_INVALID, message)],
            receipt: None,
        },
    }
}

/// Extracts the receipt from a full `pr ready --format json` document.
///
/// A standalone receipt document is deliberately rejected: the file has to be
/// the complete report the tool produced, so a hand-written token cannot pass.
///
/// # Errors
///
/// Returns an error when the document is not a readiness report carrying an
/// authorizing receipt.
pub fn extract_receipt(document: &str) -> std::result::Result<MergeReceipt, String> {
    let report: ReadinessReport =
        serde_json::from_str(document).map_err(|err| format!("not a readiness report: {err}"))?;
    let authorization = report.authorization;
    if authorization.decision != AuthorizationDecision::MergeAllowed {
        return Err(format!(
            "report authorization is {}, not MERGE_ALLOWED",
            authorization.decision.as_str()
        ));
    }
    authorization
        .receipt
        .ok_or_else(|| "MERGE_ALLOWED report carried no receipt".to_owned())
}

#[cfg(test)]
#[path = "receipt_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "receipt_digest_tests.rs"]
mod digest_tests;
