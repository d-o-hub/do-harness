//! Merge authorization: the single decision a direct merge consumes (#466).
//!
//! `ready` is presentation. The authorizing object is [`Authorization`], and the
//! only value a merge script may act on is a [`MergeReceipt`] produced by this
//! module from a complete, stable snapshot collected immediately before the
//! merge. The receipt is recomputed, never trusted: `--expected-receipt` can
//! only prove that the snapshot is unchanged and still fresh.

pub use super::reason::{
    Authorization, AuthorizationDecision, AuthorizationReason, HELD_LABELS, MergeReceipt,
    RECEIPT_SCHEMA_VERSION, codes,
};
use super::receipt::build_receipt;
pub use super::receipt::{
    ReceiptVerdict, apply_expected_receipt, extract_receipt, now_unix, unstable, validate_receipt,
};

use super::model::{CheckSummary, ConversationSummary, FindingsSummary, MergeState};
use crate::pr::ancestry::{AncestryDecision, AncestryProof};

/// Everything the authorization needs, already collected and classified.
pub struct SnapshotInputs<'a> {
    /// PR number.
    pub pr: u64,
    /// PR metadata as reported by `GitHub`.
    pub view: &'a crate::pr::gh::PrReadinessView,
    /// Merge state projection.
    pub merge_state: &'a MergeState,
    /// Classified checks.
    pub checks: &'a CheckSummary,
    /// Conversation ledger.
    pub conversations: &'a ConversationSummary,
    /// Findings summary.
    pub findings: &'a FindingsSummary,
    /// Merge policy revision, or `None` when the policy is absent/drifted.
    pub policy_revision: Option<String>,
    /// Whether the merge policy matched the server ruleset.
    pub policy_ok: bool,
    /// Whether the policy could be evaluated at all.
    pub policy_known: bool,
    /// Which source produced the policy decision.
    pub policy_source: crate::pr::policy_source::PolicySource,
    /// Ways the head's checks fail the derived ruleset.
    pub rule_violations: &'a [crate::pr::rules::RuleViolation],
    /// Ancestry proof, or `None` when it could not be collected.
    pub ancestry: Option<&'a AncestryProof>,
    /// UTC epoch seconds used to seal the receipt.
    pub now_unix: i64,
}

#[allow(clippy::too_many_lines)]
pub fn authorize(inputs: &SnapshotInputs<'_>) -> Authorization {
    let mut blocked = Vec::new();
    let mut unknown = Vec::new();
    let view = inputs.view;

    if view.head_ref_oid.is_empty() || view.base_ref_oid.is_empty() {
        unknown.push(AuthorizationReason::new(
            codes::API_INCOMPLETE,
            "the PR payload omitted a base or head commit SHA",
        ));
    }
    if view.head_ref_name.is_empty() || view.base_ref_name.is_empty() {
        unknown.push(AuthorizationReason::new(
            codes::API_INCOMPLETE,
            "the PR payload omitted a base or head branch name",
        ));
    }

    if view.state != "OPEN" {
        blocked.push(AuthorizationReason::new(
            codes::NOT_OPEN,
            format!("PR state is '{}' (expected OPEN)", view.state),
        ));
    }
    if view.is_draft {
        blocked.push(AuthorizationReason::new(
            codes::DRAFT,
            "PR is a draft; un-drafting is the author's decision",
        ));
    }
    let held: Vec<&str> = view
        .labels
        .iter()
        .map(|label| label.name.as_str())
        .filter(|name| {
            HELD_LABELS
                .iter()
                .any(|held| name.eq_ignore_ascii_case(held))
        })
        .collect();
    if !held.is_empty() {
        blocked.push(AuthorizationReason::new(
            codes::HELD,
            format!("PR carries a hard-stop label: {}", held.join(", ")),
        ));
    }
    if inputs.merge_state.status != "CLEAN" || inputs.merge_state.mergeable != "MERGEABLE" {
        blocked.push(AuthorizationReason::new(
            codes::MERGE_STATE_DIRTY,
            format!(
                "merge state is '{}' and mergeability '{}' (expected CLEAN/MERGEABLE)",
                inputs.merge_state.status, inputs.merge_state.mergeable
            ),
        ));
    }
    if inputs.merge_state.auto_merge {
        blocked.push(AuthorizationReason::new(
            codes::AUTO_MERGE_ARMED,
            "an auto-merge request is armed and would fire on an unvalidated head",
        ));
    }

    match inputs.checks.verdict.as_str() {
        "PASS" => {}
        "PENDING" => blocked.push(AuthorizationReason::new(
            codes::CHECKS_PENDING,
            "required checks are still running",
        )),
        "FAIL" => blocked.push(AuthorizationReason::new(
            codes::CHECKS_FAILED,
            format!(
                "required checks failed ({} failed, {} cancelled)",
                inputs.checks.failed.len(),
                inputs.checks.cancelled.len()
            ),
        )),
        other => unknown.push(AuthorizationReason::new(
            codes::CHECKS_UNKNOWN,
            format!("check verdict is '{other}'"),
        )),
    }
    if inputs.checks.incomplete {
        blocked.push(AuthorizationReason::new(
            codes::MERGE_STATE_DIRTY,
            "the check set is incomplete: merge conflicts stop workflows from being scheduled",
        ));
    }

    if inputs
        .view
        .review_decision
        .as_deref()
        .is_some_and(|decision| decision == "CHANGES_REQUESTED")
    {
        blocked.push(AuthorizationReason::new(
            codes::REVIEW_CHANGES_REQUESTED,
            "a reviewer requested changes",
        ));
    }
    if !inputs.conversations.unresolved_threads.is_empty() {
        blocked.push(AuthorizationReason::new(
            codes::CONVERSATION_UNRESOLVED,
            format!(
                "{} unresolved review thread(s)",
                inputs.conversations.unresolved_threads.len()
            ),
        ));
    }
    if inputs.conversations.changes_requested_reviews > 0 {
        blocked.push(AuthorizationReason::new(
            codes::REVIEW_CHANGES_REQUESTED,
            format!(
                "{} submitted review(s) request changes",
                inputs.conversations.changes_requested_reviews
            ),
        ));
    }
    if !inputs.conversations.actionable_comments.is_empty() {
        blocked.push(AuthorizationReason::new(
            codes::CONVERSATION_UNRESOLVED,
            format!(
                "{} actionable comment(s) remain unanswered",
                inputs.conversations.actionable_comments.len()
            ),
        ));
    }
    if !inputs.conversations.needs_triage.is_empty() {
        blocked.push(AuthorizationReason::new(
            codes::CONVERSATION_NEEDS_TRIAGE,
            format!(
                "{} conversation item(s) still need triage",
                inputs.conversations.needs_triage.len()
            ),
        ));
    }

    if !inputs.policy_known {
        unknown.push(AuthorizationReason::new(
            codes::POLICY_UNAVAILABLE,
            "the merge policy could not be evaluated against the server ruleset",
        ));
    } else if !inputs.policy_ok {
        // The message names the source, because "drifts from
        // plans/pr-merge-policy.toml" is wrong when no such file exists.
        let detail = match inputs.policy_source {
            crate::pr::policy_source::PolicySource::Overlay => {
                "server ruleset drifts from plans/pr-merge-policy.toml"
            }
            crate::pr::policy_source::PolicySource::Derived => {
                "the server ruleset permits no merge method the repository enables"
            }
        };
        blocked.push(AuthorizationReason::new(codes::POLICY_DRIFT, detail));
    }

    // The derived ruleset's own check gate: a missing context, a failing one,
    // or one published by an app it did not name. The last is the
    // impersonation case — the ruleset names an app for a reason.
    for violation in inputs.rule_violations {
        blocked.push(AuthorizationReason::new(
            violation.code,
            format!("{}: {}", violation.context, violation.detail),
        ));
    }

    if inputs.findings.introduced > 0 || inputs.findings.regressed > 0 {
        blocked.push(AuthorizationReason::new(
            codes::FINDINGS_UNTRACKED,
            format!(
                "{} introduced and {} regressed findings",
                inputs.findings.introduced, inputs.findings.regressed
            ),
        ));
    }
    if !inputs.findings.base_verified && inputs.findings.total > 0 {
        unknown.push(AuthorizationReason::new(
            codes::FINDINGS_UNVERIFIED_BASELINE,
            "the findings baseline could not be verified against the base commit",
        ));
    }

    match inputs.ancestry {
        Some(proof) if proof.decision.is_ancestor() => {}
        Some(proof) if proof.decision == AncestryDecision::Diverged => {
            blocked.push(AuthorizationReason::new(
                codes::ANCESTRY_DIVERGED,
                proof
                    .reason
                    .clone()
                    .unwrap_or_else(|| "head does not contain the base commit".to_owned()),
            ));
        }
        Some(_) => unknown.push(AuthorizationReason::new(
            codes::ANCESTRY_UNKNOWN,
            "ancestry could not be proven from the compare API",
        )),
        None => unknown.push(AuthorizationReason::new(
            codes::ANCESTRY_UNKNOWN,
            "the compare API was not queried",
        )),
    }

    // A known blocker settles the verdict: the merge cannot proceed whatever
    // the missing proof would have said. UNKNOWN is reserved for the case where
    // nothing known blocks but the proof needed to allow it is missing.
    if !blocked.is_empty() {
        blocked.extend(unknown);
        return Authorization {
            schema_version: RECEIPT_SCHEMA_VERSION,
            decision: AuthorizationDecision::Blocked,
            reasons: blocked,
            receipt: None,
        };
    }
    if !unknown.is_empty() {
        return Authorization {
            schema_version: RECEIPT_SCHEMA_VERSION,
            decision: AuthorizationDecision::Unknown,
            reasons: unknown,
            receipt: None,
        };
    }

    Authorization {
        schema_version: RECEIPT_SCHEMA_VERSION,
        decision: AuthorizationDecision::MergeAllowed,
        reasons: Vec::new(),
        receipt: Some(build_receipt(inputs)),
    }
}
