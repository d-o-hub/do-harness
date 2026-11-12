//! The sequential per-PR state machine (#471).
//!
//! Steps run in order: `LIST` → `AUTHORIZE_OWNER` → `NO_EFFECT` → `UPDATE_BASE` →
//! `WAIT_CI` → `REVIEW_COMMENTS`/`REVIEW_FINDINGS` (the readiness snapshot) →
//! `FINAL_READY` → `DIRECT_MERGE` → `VERIFY_MAIN` → `NEXT`. Every step composes
//! existing functions; only the direct pinned merge and the closure/disarm
//! mutations touch `GitHub`.

use std::collections::HashMap;
use std::path::Path;

use anyhow::{Context, Result};

use crate::pr::gh;
use crate::pr::readiness::{self, AuthorizationDecision, AuthorizationReason};
use crate::pr::sweep::behind;
use crate::pr::sweep::plan::{Cluster, PlannedPr};
use crate::pr::sweep::report::{HaltRecord, PerPrReport, SummaryRecord};
use crate::pr::sweep::state::{ModeRecord, SweepStateV1};
use crate::pr::sweep::step::{self, MainVerdict, Owner, WaitOutcome};

#[path = "machine_utils.rs"]
mod utils;
use utils::{
    PrCtx, cluster_map, insert_audit, merge_commit_after, receipt_path, report_for, skipped_report,
};

/// Everything the machine needs for one drive.
pub struct DriveInput<'a> {
    /// Mode of the run.
    pub mode: ModeRecord,
    /// Wait bound for CI and post-merge verification.
    pub wait_seconds: u64,
    /// Where the audit state is stored.
    pub state_path: &'a Path,
    /// Audit state (loaded and reconciled by the caller).
    pub state: &'a mut SweepStateV1,
}

/// Result of one drive.
pub struct DriveOutput {
    /// One report row per planned PR, in processing order.
    pub prs: Vec<PerPrReport>,
    /// Aggregate counts.
    pub summary: SummaryRecord,
    /// Why the whole sweep stopped, when it did.
    pub halted: Option<HaltRecord>,
}

/// Drives the sweep over the planned work list.
pub fn drive(
    root: &Path,
    planned: &[PlannedPr],
    input: &mut DriveInput<'_>,
) -> Result<DriveOutput> {
    let clusters = cluster_map(root, planned)?;
    let mut summary = SummaryRecord::default();
    let mut halted = None;
    let mut prs = Vec::with_capacity(planned.len());

    for planned in planned {
        if halted.is_some() {
            summary.skipped += 1;
            prs.push(skipped_report(&planned.entry, "halted", "sweep halted"));
            continue;
        }
        let entry = &planned.entry;
        let audit = input
            .state
            .prs
            .get(&entry.number)
            .cloned()
            .unwrap_or_default();
        let mut ctx = PrCtx {
            report: report_for(entry),
            audit,
            halt: None,
        };

        // AUTHORIZE_OWNER and the quick hard stops, from the list view.
        match early_skip(entry) {
            Some((code, message)) => {
                ctx.finish_skipped(code, &message);
            }
            None => match step::classify_author(root, entry) {
                Owner::Owned => {
                    ctx.report.authorized = true;
                    process_owned(root, &mut ctx, &clusters, &*input)?;
                }
                Owner::External => {
                    ctx.finish_skipped("external", "third-party author; never modified");
                }
                Owner::UnknownIdentity => ctx.finish_skipped(
                    "unknown_identity",
                    "author permission could not be determined",
                ),
            },
        }

        insert_audit(input.state, entry.number, &ctx.audit, &ctx.report);
        if let Err(error) = input.state.save(input.state_path) {
            eprintln!("warning: failed to save sweep state: {error}");
        }
        if let Some(halter) = &ctx.halt {
            halted = Some(halter.clone());
        }
        match ctx.report.outcome.as_str() {
            "merged" => summary.merged += 1,
            "closed_no_effect" => summary.closed_no_effect += 1,
            "closed_superseded" => summary.closed_superseded += 1,
            "blocked" => summary.blocked += 1,
            "unknown" => summary.unknown += 1,
            "deferred" => summary.deferred += 1,
            _ => summary.skipped += 1,
        }
        prs.push(ctx.report);
    }

    Ok(DriveOutput {
        prs,
        summary,
        halted,
    })
}

/// Hard stops that are visible in the list view alone.
fn early_skip(entry: &crate::pr::gh::PrListEntry) -> Option<(&'static str, String)> {
    if entry.state != "OPEN" {
        return Some(("closed", format!("PR state is '{}'", entry.state)));
    }
    if entry.is_draft {
        return Some((
            "draft",
            "PR is a draft; un-drafting is the author's decision".to_owned(),
        ));
    }
    if let Some(label) = step::held_label(&entry.labels) {
        return Some(("held", format!("PR carries a hard-stop label: {label}")));
    }
    if entry.merge_state_status == "DIRTY" {
        return Some(("conflict", "merge state is DIRTY".to_owned()));
    }
    None
}

/// Processes an owned PR through the remaining steps.
#[allow(clippy::too_many_lines)]
fn process_owned(
    root: &Path,
    ctx: &mut PrCtx,
    clusters: &HashMap<u64, Cluster>,
    input: &DriveInput<'_>,
) -> Result<()> {
    let number = ctx.report.number;
    let Some(mut view) = utils::preflight(root, ctx, clusters, input) else {
        return Ok(());
    };

    // UPDATE_BASE: only behind PRs, pinned to the captured head. The recovery
    // chain itself lives in `behind` (#518) so this step reads as the decision
    // it makes.
    if view.merge_state_status == "BEHIND" {
        if !input.mode.execute {
            ctx.finish_blocked(vec![AuthorizationReason::new(
                "update_required",
                "branch is behind its base; update requires --execute",
            )]);
            return Ok(());
        }
        let old_head = view.head_ref_oid.clone();
        let author = ctx.report.author.clone();
        let wait = input.wait_seconds.min(behind::MAX_UPDATE_WAIT_SECONDS);
        match behind::update(
            root,
            number,
            &old_head,
            &author,
            Owner::Owned,
            &mut ctx.audit,
            wait,
        ) {
            behind::Outcome::Updated {
                view: refreshed,
                via,
            } => {
                view = *refreshed;
                ctx.report.branch_update = Some(via);
            }
            behind::Outcome::Stalled { via } => {
                ctx.report.branch_update = Some(via);
                ctx.finish_unknown(
                    "update_timeout",
                    "branch head did not change within the update deadline",
                );
                return Ok(());
            }
            behind::Outcome::Unupdatable { via, reason } => {
                ctx.report.branch_update = Some(via);
                ctx.finish_blocked(vec![reason]);
                return Ok(());
            }
            behind::Outcome::ReadFailed => {
                ctx.finish_unknown("api_incomplete", "could not re-read the PR after update");
                return Ok(());
            }
        }
        if view.merge_state_status == "DIRTY" || view.mergeable == "CONFLICTING" {
            ctx.finish_skipped("conflict", "update left the PR conflicting");
            return Ok(());
        }
    }

    // WAIT_CI.
    let conflicts = view.merge_state_status == "DIRTY" || view.mergeable == "CONFLICTING";
    match step::wait_for_checks(
        root,
        number,
        &view.head_ref_oid,
        conflicts,
        input.wait_seconds,
    ) {
        WaitOutcome::Passed => {}
        WaitOutcome::Failed(report) => {
            ctx.finish_blocked(vec![AuthorizationReason::new(
                crate::pr::readiness::codes::CHECKS_FAILED,
                format!(
                    "required checks failed ({} blocker(s))",
                    report.blockers.len()
                ),
            )]);
            return Ok(());
        }
        WaitOutcome::Unknown => {
            ctx.finish_unknown(
                crate::pr::readiness::codes::CHECKS_UNKNOWN,
                "check verdict is unknown",
            );
            return Ok(());
        }
        WaitOutcome::Deadline => {
            ctx.finish_unknown(
                "checks_pending_deadline",
                "checks were still pending when the wait bound expired",
            );
            return Ok(());
        }
    }

    // Evidence snapshot (REVIEW_COMMENTS / REVIEW_FINDINGS / policy / ancestry).
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
    ctx.report.head_sha = Some(view.head_ref_oid.clone());
    match snapshot.authorization.decision {
        AuthorizationDecision::Blocked => {
            ctx.finish_blocked(snapshot.authorization.reasons);
            return Ok(());
        }
        AuthorizationDecision::Unknown => {
            "unknown".clone_into(&mut ctx.report.outcome);
            ctx.report.reasons = snapshot.authorization.reasons;
            ctx.audit.outcome = Some("unknown".to_owned());
            return Ok(());
        }
        AuthorizationDecision::MergeAllowed => {}
    }

    // FINAL_READY: re-collect against the receipt immediately before merge.
    if !input.mode.merge {
        "deferred".clone_into(&mut ctx.report.outcome);
        ctx.report.next_action = Some("rerun with --execute --merge to merge".to_owned());
        ctx.audit.outcome = Some("deferred".to_owned());
        return Ok(());
    }
    let receipt_path = receipt_path(input.state_path, number);
    let receipt =
        serde_json::to_string_pretty(&snapshot).context("serialize readiness snapshot")?;
    std::fs::write(&receipt_path, receipt).context("write receipt file")?;
    let final_snapshot = match readiness::evaluate_with_receipt(root, number, Some(&receipt_path)) {
        Ok(snapshot) => snapshot,
        Err(error) => {
            ctx.finish_unknown("api_incomplete", &format!("{error:#}"));
            return Ok(());
        }
    };
    match final_snapshot.authorization.decision {
        AuthorizationDecision::MergeAllowed => {}
        AuthorizationDecision::Blocked => {
            ctx.finish_blocked(final_snapshot.authorization.reasons);
            return Ok(());
        }
        AuthorizationDecision::Unknown => {
            "unknown".clone_into(&mut ctx.report.outcome);
            ctx.report.reasons = final_snapshot.authorization.reasons;
            ctx.audit.outcome = Some("unknown".to_owned());
            return Ok(());
        }
    }
    let head_sha = final_snapshot.head_ref_oid.clone();

    // DIRECT_MERGE: the single pinned command.
    let merge_sha = match gh::merge(root, number, &head_sha) {
        Ok(()) => merge_commit_after(root, number),
        Err(error) => {
            // Reconcile an uncertain mutation: read the actual state.
            match gh::merged_view(root, number) {
                Ok(mv) if mv.state == "MERGED" => mv.merge_commit.map(|mc| mc.oid),
                _ => {
                    ctx.finish_blocked(vec![AuthorizationReason::new(
                        "merge_refused",
                        format!("{error:#}"),
                    )]);
                    return Ok(());
                }
            }
        }
    };
    let Some(merge_sha) = merge_sha.filter(|sha| !sha.trim().is_empty()) else {
        ctx.finish_unknown(
            "snapshot_moved",
            "merge completed without a recorded merge commit",
        );
        return Ok(());
    };
    ctx.report.merge_sha = Some(merge_sha.clone());
    ctx.audit.merge_sha = Some(merge_sha.clone());

    // VERIFY_MAIN.
    match step::verify_main_after_merge(root, &merge_sha, input.wait_seconds) {
        MainVerdict::Pass => {
            ctx.report.post_merge = Some("pass".to_owned());
            ctx.audit.post_merge = Some("pass".to_owned());
        }
        MainVerdict::Regression => {
            ctx.report.post_merge = Some("regression".to_owned());
            ctx.audit.post_merge = Some("regression".to_owned());
            ctx.halt = Some(HaltRecord {
                code: "regression".to_owned(),
                message: "post-merge main verification failed; open a focused revert PR and halt"
                    .to_owned(),
                pr: number,
            });
        }
        MainVerdict::Unknown => {
            ctx.report.post_merge = Some("unknown".to_owned());
            ctx.audit.post_merge = Some("unknown".to_owned());
            ctx.halt = Some(HaltRecord {
                code: "unknown".to_owned(),
                message: "post-merge main verification unknown at the deadline; halt".to_owned(),
                pr: number,
            });
        }
    }
    "merged".clone_into(&mut ctx.report.outcome);
    ctx.audit.outcome = Some("merged".to_owned());
    Ok(())
}
