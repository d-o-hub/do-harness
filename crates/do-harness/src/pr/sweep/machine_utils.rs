//! Helper routines for the sweep machine, split out for the LOC ceiling.

use std::collections::HashMap;
use std::path::Path;

use anyhow::Result;

use crate::pr::gh::{self, PrReadinessView};
use crate::pr::no_effect::merge_impact_for_pr;
use crate::pr::readiness::AuthorizationReason;
use crate::pr::sweep::plan::{Cluster, PlannedPr};
use crate::pr::sweep::report::{ClusterRecord, HaltRecord, PerPrReport};
use crate::pr::sweep::state::{PrAudit, SweepStateV1};
use crate::pr::sweep::step::{self, Owner};

/// Reads the merge commit after a merge command, reconciling eventual
/// consistency through one short bounded poll.
pub(super) fn merge_commit_after(root: &Path, number: u64) -> Option<String> {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    loop {
        match gh::merged_view(root, number) {
            Ok(mv) if mv.state == "MERGED" => {
                if let Some(mc) = mv.merge_commit {
                    if !mc.oid.trim().is_empty() {
                        return Some(mc.oid);
                    }
                }
            }
            _ => {}
        }
        if std::time::Instant::now() >= deadline {
            return None;
        }
        std::thread::sleep(std::time::Duration::from_millis(500));
    }
}

/// Receipt file path next to the state file.
pub(super) fn receipt_path(state_path: &Path, number: u64) -> std::path::PathBuf {
    let parent = state_path.parent().unwrap_or_else(|| Path::new("."));
    parent.join(format!("receipt-pr-{number}.json"))
}

/// Writes the audit record into the state.
pub(super) fn insert_audit(
    state: &mut SweepStateV1,
    number: u64,
    audit: &PrAudit,
    report: &PerPrReport,
) {
    let mut audit = audit.clone();
    audit.head_sha = Some(report.head_sha.clone().unwrap_or_default());
    audit.base_sha = Some(report.base_sha.clone().unwrap_or_default());
    state.prs.insert(number, audit);
}

/// Builds a report row for an early-skip PR.
pub(super) fn skipped_report(
    entry: &crate::pr::gh::PrListEntry,
    code: &str,
    message: &str,
) -> PerPrReport {
    let mut report = report_for(entry);
    "skipped".clone_into(&mut report.outcome);
    report.reasons.push(AuthorizationReason::new(code, message));
    report
}

/// Builds the base report row for a PR.
pub(super) fn report_for(entry: &crate::pr::gh::PrListEntry) -> PerPrReport {
    PerPrReport {
        number: entry.number,
        title: entry.title.clone(),
        url: entry.url.clone(),
        author: entry
            .author
            .as_ref()
            .map(|author| author.login.clone())
            .unwrap_or_default(),
        authorized: false,
        outcome: String::new(),
        reasons: Vec::new(),
        blockers: Vec::new(),
        next_action: None,
        base_sha: None,
        head_sha: None,
        branch_update: None,
        checks: None,
        conversations: None,
        findings: None,
        merge_sha: None,
        post_merge: None,
        cluster: None,
    }
}

/// Maps keeper/loser roles for owned PRs.
pub(super) fn cluster_map(root: &Path, planned: &[PlannedPr]) -> Result<HashMap<u64, Cluster>> {
    let owned: Vec<PlannedPr> = planned
        .iter()
        .filter(|planned| matches!(step::classify_author(root, &planned.entry), Owner::Owned))
        .cloned()
        .collect();
    let mut map = HashMap::new();
    for cluster in crate::pr::sweep::plan::find_clusters(root, &owned)? {
        map.insert(cluster.keeper, cluster.clone());
        for loser in &cluster.losers {
            map.insert(*loser, cluster.clone());
        }
    }
    Ok(map)
}

/// Mutable per-PR context.
pub(super) struct PrCtx {
    pub(super) report: PerPrReport,
    pub(super) audit: PrAudit,
    pub(super) halt: Option<HaltRecord>,
}

impl PrCtx {
    pub(super) fn finish_skipped(&mut self, code: &str, message: &str) {
        "skipped".clone_into(&mut self.report.outcome);
        self.report
            .reasons
            .push(AuthorizationReason::new(code, message));
        self.audit.outcome = Some("skipped".to_owned());
    }

    pub(super) fn finish_blocked(&mut self, reasons: Vec<AuthorizationReason>) {
        "blocked".clone_into(&mut self.report.outcome);
        self.report.next_action = step::next_action(self.report.number, &reasons);
        self.report.reasons = reasons;
        self.audit.outcome = Some("blocked".to_owned());
    }

    pub(super) fn finish_unknown(&mut self, code: &str, message: &str) {
        "unknown".clone_into(&mut self.report.outcome);
        self.report
            .reasons
            .push(AuthorizationReason::new(code, message));
        self.audit.outcome = Some("unknown".to_owned());
    }
}

/// Everything that must pass before CI is worth waiting for.
///
/// Returns `Some(view)` when the PR continues to the update/CI/merge steps, and
/// `None` when this step already finished it — skipped, blocked, or closed. The
/// caller cannot tell those apart by outcome here and must not need to: the
/// report row already carries the reason.
pub(super) fn preflight(
    root: &Path,
    ctx: &mut PrCtx,
    clusters: &HashMap<u64, Cluster>,
    input: &super::DriveInput<'_>,
) -> Option<PrReadinessView> {
    let number = ctx.report.number;
    let mut view = match gh::readiness_view(root, number) {
        Ok(view) => view,
        Err(error) => {
            ctx.finish_unknown("api_incomplete", &format!("{error:#}"));
            return None;
        }
    };
    // Hard stops first: an armed auto-merge or a duplicate closure would
    // otherwise mutate a PR that should never have been touched.
    if !quick_stops(ctx, &view) {
        return None;
    }
    if !disarm_auto_merge(root, ctx, input, &mut view) {
        return None;
    }
    if !resolve_cluster(root, ctx, clusters, input) {
        return None;
    }
    no_effect(root, ctx, input, &view).then_some(view)
}

/// The stops visible in the full view but not in the list.
fn quick_stops(ctx: &mut PrCtx, view: &PrReadinessView) -> bool {
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
    if view.mergeable == "CONFLICTING" || view.merge_state_status == "DIRTY" {
        ctx.finish_skipped("conflict", "PR conflicts with its base");
        return false;
    }
    true
}

/// Disarms a pre-existing auto-merge request before any other disposition.
///
/// An armed request fires on whatever head exists when `GitHub` next computes
/// mergeability, so it must not survive a cluster/no-effect decision. Returns
/// `true` when the PR may continue.
fn disarm_auto_merge(
    root: &Path,
    ctx: &mut PrCtx,
    input: &super::DriveInput<'_>,
    view: &mut PrReadinessView,
) -> bool {
    if view.auto_merge_request.is_none() {
        return true;
    }
    if !input.mode.execute {
        ctx.finish_blocked(vec![AuthorizationReason::new(
            crate::pr::readiness::codes::AUTO_MERGE_ARMED,
            "an auto-merge request is armed; disarm requires --execute",
        )]);
        return false;
    }
    if let Err(error) = gh::disable_auto_merge(root, ctx.report.number) {
        ctx.finish_blocked(vec![AuthorizationReason::new(
            "disarm_failed",
            format!("auto-merge disarm failed: {error:#}"),
        )]);
        return false;
    }
    ctx.audit.disarmed_auto_merge = true;
    match gh::readiness_view(root, ctx.report.number) {
        Ok(refreshed) if refreshed.auto_merge_request.is_none() => {
            *view = refreshed;
            true
        }
        _ => {
            ctx.finish_blocked(vec![AuthorizationReason::new(
                "disarm_failed",
                "auto-merge is still armed after disarm",
            )]);
            false
        }
    }
}

/// Handles duplicate-cluster roles: close losers, mark keepers.
///
/// Returns `false` when the PR is finished here (a loser, closed or blocked).
fn resolve_cluster(
    root: &Path,
    ctx: &mut PrCtx,
    clusters: &HashMap<u64, Cluster>,
    input: &super::DriveInput<'_>,
) -> bool {
    let number = ctx.report.number;
    let Some(cluster) = clusters.get(&number) else {
        return true;
    };
    if !cluster.losers.contains(&number) {
        ctx.report.cluster = Some(ClusterRecord {
            role: "keeper".to_owned(),
            keeper: None,
            rule: Some(cluster.rule.to_owned()),
        });
        return true;
    }
    ctx.report.cluster = Some(ClusterRecord {
        role: "loser".to_owned(),
        keeper: Some(cluster.keeper),
        rule: Some(cluster.rule.to_owned()),
    });
    if !input.mode.merge {
        ctx.finish_blocked(vec![AuthorizationReason::new(
            "execute_merge_required",
            "duplicate supersede closure requires --execute --merge",
        )]);
        return false;
    }
    let files = match gh::pr_file_set(root, number) {
        Ok(files) => files,
        Err(error) => {
            ctx.finish_unknown("api_incomplete", &format!("{error:#}"));
            return false;
        }
    };
    match gh::close_with_comment(
        root,
        number,
        &step::supersede_comment(cluster.keeper, &files, cluster.rule),
    ) {
        Ok(()) => {
            "closed_superseded".clone_into(&mut ctx.report.outcome);
            ctx.audit.outcome = Some("closed_superseded".to_owned());
        }
        Err(error) => ctx.finish_blocked(vec![AuthorizationReason::new(
            "close_refused",
            format!("{error:#}"),
        )]),
    }
    false
}

/// Closes a proven, base-verified no-tree-effect PR.
///
/// Returns `true` when the PR continues to the merge steps.
fn no_effect(
    root: &Path,
    ctx: &mut PrCtx,
    input: &super::DriveInput<'_>,
    view: &PrReadinessView,
) -> bool {
    let number = ctx.report.number;
    let impact = merge_impact_for_pr(
        root,
        &view.base_ref_oid,
        &view.base_ref_name,
        &view.head_ref_oid,
    );
    if !step::closure_eligible(&impact) {
        return true;
    }
    let base = impact.base_sha.as_deref().unwrap_or_default().to_owned();
    let head = impact.head_sha.as_deref().unwrap_or_default().to_owned();
    let unchanged = matches!(
        gh::readiness_view(root, number),
        Ok(view) if view.base_ref_oid == base && view.head_ref_oid == head
    );
    if !unchanged {
        ctx.finish_blocked(vec![AuthorizationReason::new(
            crate::pr::readiness::codes::SNAPSHOT_MOVED,
            "base/head moved after the no-effect proof; closure refused",
        )]);
        return false;
    }
    if !input.mode.merge {
        ctx.finish_blocked(vec![AuthorizationReason::new(
            "execute_merge_required",
            "no-tree-effect closure requires --execute --merge",
        )]);
        return false;
    }
    match gh::close_with_comment(root, number, &step::no_effect_comment(&impact)) {
        Ok(()) => {
            "closed_no_effect".clone_into(&mut ctx.report.outcome);
            ctx.report.base_sha = Some(base);
            ctx.report.head_sha = Some(head);
            ctx.audit.outcome = Some("closed_no_effect".to_owned());
        }
        Err(error) => ctx.finish_blocked(vec![AuthorizationReason::new(
            "close_refused",
            format!("{error:#}"),
        )]),
    }
    false
}
