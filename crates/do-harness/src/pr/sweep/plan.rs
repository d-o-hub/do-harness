//! Work-list construction for the sequential PR sweep (#471).
//!
//! The inventory is one read-only `gh pr list` (or `gh pr view` for a
//! single-target sweep). Ordering follows the skill's merge-order reference:
//! stack parents before their children, then oldest-first by creation time,
//! then number as a deterministic tiebreak. Duplicate clusters are detected
//! strictly: identical changed-file sets *and* the same base OID, which
//! guarantees the same base blobs. The keeper rules from the duplicate-clusters
//! reference are applied only to authorized PRs; an indistinguishable cluster
//! escalates instead of closing anything.

use std::collections::HashMap;
use std::path::Path;

use anyhow::Result;

use crate::pr::checks;
use crate::pr::gh::{self, PrListEntry};

/// One planned PR with its ordering rank.
#[derive(Debug, Clone)]
pub struct PlannedPr {
    /// List-shaped metadata for the PR.
    pub entry: PrListEntry,
    /// Deterministic processing rank (0 = first).
    #[allow(dead_code)]
    pub rank: usize,
}

/// A duplicate cluster with its chosen keeper.
#[derive(Debug, Clone)]
pub struct Cluster {
    /// The keeper PR number.
    pub keeper: u64,
    /// The other members, to be closed as superseded.
    pub losers: Vec<u64>,
    /// Which documented keeper rule picked the keeper: `a` (green beats red),
    /// `d` (newest rebase), or `tie` (escalation, nothing is closed).
    pub rule: &'static str,
}

/// Sorts the open-PR inventory into merge order.
///
/// A PR is a stack child when its base branch is another open PR's head
/// branch; children sort after their parents. Independent PRs sort by
/// `createdAt` then number. The sort is stable, so equal keys keep the list's
/// original (API) order.
pub fn ordered_worklist(entries: Vec<PrListEntry>) -> Vec<PlannedPr> {
    let heads: std::collections::HashSet<String> = entries
        .iter()
        .map(|entry| entry.head_ref_name.clone())
        .collect();
    let mut sorted = entries;
    sorted.sort_by(|a, b| {
        let a_child = heads.contains(&a.base_ref_name);
        let b_child = heads.contains(&b.base_ref_name);
        a_child
            .cmp(&b_child)
            .then_with(|| a.created_at.cmp(&b.created_at))
            .then_with(|| a.number.cmp(&b.number))
    });
    sorted
        .into_iter()
        .enumerate()
        .map(|(rank, entry)| PlannedPr { entry, rank })
        .collect()
}

/// Finds duplicate clusters among authorized PRs.
///
/// Members must share the same changed-file set *and* the same base OID (the
/// same base commit implies the same base blobs). The keeper is the first
/// separating rule that applies: green beats red, then the most recently
/// updated branch (the cheapest rebase proxy). When members are still
/// indistinguishable the cluster is reported with `rule: "tie"` and nothing
/// is closed.
///
/// # Errors
///
/// Returns an error when a PR's file set cannot be read; the caller treats
/// that as unavailable evidence, never as an empty cluster set.
pub fn find_clusters(root: &Path, owned: &[PlannedPr]) -> Result<Vec<Cluster>> {
    let mut groups: HashMap<(String, Vec<String>), Vec<&PlannedPr>> = HashMap::new();
    for planned in owned {
        let files = gh::pr_file_set(root, planned.entry.number)?;
        groups
            .entry((planned.entry.base_ref_oid.clone(), files))
            .or_default()
            .push(planned);
    }
    let mut clusters = Vec::new();
    for members in groups.values() {
        if members.len() < 2 {
            continue;
        }
        let mut best: Option<&PlannedPr> = None;
        let mut best_green: Option<bool> = None;
        let mut greens: HashMap<u64, bool> = HashMap::new();
        for member in members {
            let green = checks_green(root, member);
            greens.insert(member.entry.number, green);
            let take = match (best, best_green) {
                (Some(current), Some(current_green)) => {
                    (green && !current_green)
                        || (green == current_green
                            && member.entry.updated_at > current.entry.updated_at)
                }
                _ => true,
            };
            if take {
                best = Some(member);
                best_green = Some(green);
            }
        }
        let Some(keeper_planned) = best else {
            continue;
        };
        let keeper = keeper_planned.entry.number;
        let keeper_green = greens.get(&keeper).copied().unwrap_or(false);
        // The rule is derived from the *selected* keeper, not from whichever
        // comparison happened to run last: a first-member keeper is still a
        // determinate winner when every other member is older.
        let rule = if greens.values().any(|green| *green != keeper_green) {
            "a"
        } else if members.iter().any(|member| {
            member.entry.number != keeper
                && member.entry.updated_at != keeper_planned.entry.updated_at
        }) {
            "d"
        } else {
            "tie"
        };
        let losers: Vec<u64> = members
            .iter()
            .map(|member| member.entry.number)
            .filter(|number| *number != keeper)
            .collect();
        if rule != "tie" {
            clusters.push(Cluster {
                keeper,
                losers,
                rule,
            });
        }
    }
    Ok(clusters)
}

/// Whether a planned PR's checks currently classify as a full pass.
fn checks_green(root: &Path, planned: &PlannedPr) -> bool {
    matches!(
        checks::evaluate_for_head(root, planned.entry.number, &planned.entry.head_ref_oid, false),
        Ok(report) if report.verdict == checks::Verdict::Pass
    )
}
