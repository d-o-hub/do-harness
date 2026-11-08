//! Helper routines for the sweep machine, split out for the LOC ceiling.

use std::collections::HashMap;
use std::path::Path;

use anyhow::Result;

use crate::pr::gh::{self, PrReadinessView};
use crate::pr::readiness::AuthorizationReason;
use crate::pr::sweep::plan::{Cluster, PlannedPr};
use crate::pr::sweep::report::{HaltRecord, PerPrReport};
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

/// Result of waiting for an update-branch head change.
pub(super) enum HeadPoll {
    Changed(Box<PrReadinessView>),
    Unchanged,
    ReadFailed,
}

/// Waits for the head to change after an update-branch request, observing
/// immediately and never before the first read.
pub(super) fn wait_for_head_change(
    root: &Path,
    number: u64,
    old_head: &str,
    wait_seconds: u64,
) -> HeadPoll {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(wait_seconds);
    loop {
        match gh::readiness_view(root, number) {
            Ok(view) => {
                if view.head_ref_oid != old_head {
                    return HeadPoll::Changed(Box::new(view));
                }
            }
            Err(_) => return HeadPoll::ReadFailed,
        }
        if std::time::Instant::now() >= deadline {
            return HeadPoll::Unchanged;
        }
        std::thread::sleep(std::time::Duration::from_secs(3));
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
