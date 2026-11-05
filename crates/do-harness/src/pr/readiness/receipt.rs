//! Receipt construction, validation, and application.

use std::collections::BTreeMap;

use sha2::{Digest, Sha256};

use super::authorization::SnapshotInputs;
use super::model::{CheckSummary, ConversationSummary, FindingsSummary, ReadinessReport};
use super::reason::{
    Authorization, AuthorizationDecision, AuthorizationReason, MergeReceipt,
    RECEIPT_LIFETIME_SECONDS, RECEIPT_SCHEMA_VERSION, codes,
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
        ("ancestry", ancestry.clone()),
        ("ancestry_base", ancestry_base.clone()),
        ("ancestry_merge_base", ancestry_merge_base.clone()),
    ]);
    for (name, value) in &pairs {
        hash_field(&mut hasher, name, value);
    }
    let snapshot_sha256 = sha256_hex(&hasher.finalize());

    MergeReceipt {
        schema_version: RECEIPT_SCHEMA_VERSION,
        pr: inputs.pr,
        issued_at_unix: inputs.now_unix,
        expires_at_unix: inputs.now_unix + RECEIPT_LIFETIME_SECONDS,
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
    if supplied.expires_at_unix <= supplied.issued_at_unix
        || supplied.expires_at_unix - supplied.issued_at_unix > RECEIPT_LIFETIME_SECONDS
    {
        return ReceiptVerdict::Invalid(
            "receipt lifetime is not the declared 60 seconds".to_owned(),
        );
    }
    if supplied.issued_at_unix > now_unix + RECEIPT_LIFETIME_SECONDS {
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
                    "receipt expired at unix {} (now {now_unix})",
                    supplied.expires_at_unix
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
mod tests {
    use crate::pr::readiness::reason::{RECEIPT_LIFETIME_SECONDS, RECEIPT_SCHEMA_VERSION};

    use super::{MergeReceipt, ReceiptVerdict, validate_receipt};

    #[test]
    fn receipt_lifetime_is_sixty_seconds() {
        assert_eq!(RECEIPT_LIFETIME_SECONDS, 60);
    }

    #[test]
    fn a_wrong_pr_number_is_an_invalid_receipt() {
        let refreshed = MergeReceipt {
            schema_version: RECEIPT_SCHEMA_VERSION,
            pr: 7,
            issued_at_unix: 100,
            expires_at_unix: 160,
            snapshot_sha256: "abc".to_owned(),
            base_ref_name: "main".to_owned(),
            base_ref_oid: "b".to_owned(),
            head_ref_name: "topic".to_owned(),
            head_ref_oid: "h".to_owned(),
            head_repository: Some("acme/widget".to_owned()),
            state: "OPEN".to_owned(),
            is_draft: false,
            labels: Vec::new(),
            mergeable: "MERGEABLE".to_owned(),
            merge_state_status: "CLEAN".to_owned(),
            auto_merge: false,
            review_decision: None,
            checks_sha256: String::new(),
            conversations_sha256: String::new(),
            findings_sha256: String::new(),
            policy_revision: "absent".to_owned(),
            ancestry: "ahead".to_owned(),
            ancestry_base_commit: "b".to_owned(),
            ancestry_merge_base: "b".to_owned(),
        };
        let mut supplied = refreshed.clone();
        supplied.pr = 8;
        assert!(matches!(
            validate_receipt(&supplied, &refreshed, 120),
            ReceiptVerdict::Invalid(_)
        ));
    }

    #[test]
    fn an_expired_receipt_is_expired_not_changed() {
        let refreshed = MergeReceipt {
            schema_version: RECEIPT_SCHEMA_VERSION,
            pr: 7,
            issued_at_unix: 100,
            expires_at_unix: 160,
            snapshot_sha256: "abc".to_owned(),
            base_ref_name: "main".to_owned(),
            base_ref_oid: "b".to_owned(),
            head_ref_name: "topic".to_owned(),
            head_ref_oid: "h".to_owned(),
            head_repository: Some("acme/widget".to_owned()),
            state: "OPEN".to_owned(),
            is_draft: false,
            labels: Vec::new(),
            mergeable: "MERGEABLE".to_owned(),
            merge_state_status: "CLEAN".to_owned(),
            auto_merge: false,
            review_decision: None,
            checks_sha256: String::new(),
            conversations_sha256: String::new(),
            findings_sha256: String::new(),
            policy_revision: "absent".to_owned(),
            ancestry: "ahead".to_owned(),
            ancestry_base_commit: "b".to_owned(),
            ancestry_merge_base: "b".to_owned(),
        };
        assert_eq!(
            validate_receipt(&refreshed, &refreshed, 161),
            ReceiptVerdict::Expired
        );
        let mut moved = refreshed.clone();
        moved.snapshot_sha256 = "def".to_owned();
        assert_eq!(
            validate_receipt(&refreshed, &moved, 120),
            ReceiptVerdict::Changed
        );
    }
}

#[cfg(test)]
mod digest_tests {
    use crate::pr::checks::{CheckEntry, Verdict};
    use crate::pr::readiness::model::CheckSummary;

    use super::checks_digest;

    fn check_summary(verdict: &str) -> CheckSummary {
        CheckSummary {
            total: 1,
            passed: 1,
            failed: Vec::new(),
            cancelled: Vec::new(),
            pending: Vec::new(),
            skipped: 0,
            incomplete: false,
            verdict: verdict.to_owned(),
            head_sha: "h".to_owned(),
            unknown: Vec::new(),
            entries: vec![CheckEntry {
                name: "verify".to_owned(),
                source: "github-actions".to_owned(),
                kind: "check".to_owned(),
                state: "completed".to_owned(),
                conclusion: Some("success".to_owned()),
                verdict: Verdict::Pass,
                reason: None,
                rerun_command: None,
                url: None,
            }],
            blockers: Vec::new(),
        }
    }

    #[test]
    fn checks_digest_changes_when_a_check_verdict_changes() {
        let mut summary = check_summary("PASS");
        let before = checks_digest(&summary);
        summary.verdict = "FAIL".to_owned();
        assert_ne!(before, checks_digest(&summary));
    }
}
