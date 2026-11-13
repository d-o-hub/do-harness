//! The external-author merge path (#519).
//!
//! Under [`crate::pr::author_policy::AuthorPolicy::OwnedOrExternalGreen`] an
//! external contributor's pull request may merge, but the permission is exactly
//! one-way: this module reads the snapshot and merges. It never updates,
//! comments on, force-pushes, or resolves a thread on a branch it does not own.
//!
//! That boundary is structural rather than conditional: the owned path's write
//! helpers (`update-branch`, auto-merge disarm, duplicate-cluster closure,
//! no-effect closure, the rebase comment) are simply not called here, so no
//! future edit can widen the policy into a licence to rewrite someone's branch.

use std::path::Path;

use anyhow::Result;

use crate::pr::gh;
use crate::pr::readiness::{self, AuthorizationDecision, AuthorizationReason};
use crate::pr::sweep::step;

use super::DriveInput;
use super::finish_and_merge;
use super::utils::PrCtx;

/// Evaluates an external author's pull request for merge under the relaxed
/// policy (#519), without ever writing to its branch.
///
/// The gate is the readiness snapshot itself, so "green" means exactly what it
/// means for an owned pull request: required checks green per #515/#516, no
/// unresolved thread, no `CHANGES_REQUESTED`, no untracked introduced finding,
/// merge state `CLEAN`/`MERGEABLE`, ancestry proved. Nothing is relaxed by the
/// author being external.
///
/// The write steps are skipped on purpose rather than made conditional: no
/// `update-branch`, no auto-merge disarm, no closure, no rebase comment. A
/// branch this tool does not own is a branch it may only read and merge — and a
/// pull request that needs one of those steps is reported instead, so the author
/// or a maintainer can act.
pub(super) fn process_external(root: &Path, ctx: &mut PrCtx, input: &DriveInput<'_>) -> Result<()> {
    let number = ctx.report.number;
    let view = match gh::readiness_view(root, number) {
        Ok(view) => view,
        Err(error) => {
            ctx.finish_unknown("api_incomplete", &format!("{error:#}"));
            return Ok(());
        }
    };
    if !passes_hard_stops(ctx, &view) {
        return Ok(());
    }
    let snapshot = match readiness::evaluate_with_receipt(root, number, None) {
        Ok(snapshot) => snapshot,
        Err(error) => {
            ctx.finish_unknown("api_incomplete", &format!("{error:#}"));
            return Ok(());
        }
    };
    ctx.report.checks = Some(snapshot.checks.clone());
    ctx.report.conversations = Some(snapshot.conversations.clone());
    ctx.report.findings = Some(snapshot.findings.clone());
    ctx.report.base_sha = Some(view.base_ref_oid.clone());
    ctx.report.head_sha = Some(snapshot.head_ref_oid.clone());
    match snapshot.authorization.decision {
        AuthorizationDecision::MergeAllowed => {}
        AuthorizationDecision::Blocked => {
            // The findings are reported, never acted on: a blocker on a branch
            // this tool does not own is the author's or a maintainer's work.
            let mut reasons = snapshot.authorization.reasons;
            reasons.push(AuthorizationReason::new(
                "external_blocked",
                "an external pull request is mergeable only when fully green; its branch is never \
                 updated, commented on, or pushed",
            ));
            "skipped".clone_into(&mut ctx.report.outcome);
            ctx.report.reasons = reasons;
            ctx.audit.outcome = Some("skipped".to_owned());
            return Ok(());
        }
        AuthorizationDecision::Unknown => {
            "unknown".clone_into(&mut ctx.report.outcome);
            ctx.report.reasons = snapshot.authorization.reasons;
            ctx.audit.outcome = Some("unknown".to_owned());
            return Ok(());
        }
    }
    finish_and_merge(root, ctx, input, &snapshot)
}

/// The stops that stay hard for every author, whichever policy is in force.
///
/// A draft, a `hold`/`wip`/`do-not-merge` label, and an armed auto-merge are the
/// author's or a maintainer's own decisions: relaxing *who may merge* is not a
/// licence to override them. An armed request is reported rather than disarmed,
/// because disarming it would be a write on someone else's branch.
///
/// A `BEHIND` branch is reported as `external_behind` rather than left to the
/// snapshot, whose only vocabulary for it is `merge_state_dirty` — the honest
/// answer is that only the author can move the branch.
///
/// Returns `false` when the pull request is finished here.
fn passes_hard_stops(ctx: &mut PrCtx, view: &gh::PrReadinessView) -> bool {
    if view.state != "OPEN" {
        ctx.finish_skipped("closed", &format!("PR state is '{}'", view.state));
        return false;
    }
    if view.is_draft {
        ctx.finish_skipped(
            "draft",
            "PR is a draft; un-drafting is the author's decision",
        );
        return false;
    }
    if let Some(label) = step::held_label(&view.labels) {
        ctx.finish_skipped("held", &format!("PR carries a hard-stop label: {label}"));
        return false;
    }
    if view.auto_merge_request.is_some() {
        ctx.finish_skipped(
            "external_auto_merge_armed",
            "an auto-merge request is armed on an external branch; this tool never disarms it",
        );
        return false;
    }
    if view.merge_state_status == "BEHIND" {
        ctx.finish_skipped(
            "external_behind",
            "the branch is behind its base; only its author can update it",
        );
        return false;
    }
    true
}
