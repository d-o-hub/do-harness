//! `pr no-effect`: the merge-impact proof, and the prefetch that makes it
//! possible in a shallow clone (#520).
//!
//! The proof compares two commits, so both have to exist locally. On a full
//! clone they do. In a fresh or shallow clone they usually do not, and the
//! honest verdict is then [`MergeImpact::Unknown`] — correct, but useless: the
//! operator asked whether the PR changes the base tree, and got "I have nothing
//! to compare". [`super::super::fetch`] closes that gap with the smallest fetch
//! that answers the question.
//!
//! # Why the base is re-read after a fetch
//!
//! A fetch takes time, and `main` can move while it does. Proving
//! `NO_TREE_EFFECT` against the base we asked for, and then reporting it as a
//! fact about the *current* base, would be a proof of the wrong thing. So after
//! a fetch the view is read again and the base must be unchanged; a base that
//! moved means the branch is behind and nothing is proven.

use std::path::Path;

use anyhow::{Context, Result};
use serde::Serialize;

use crate::pr::fetch::{self, PrefetchOutcome};
use crate::pr::gh::{self, PrView};
use crate::pr::no_effect::{self, Effect, MergeImpactReport};
use crate::report::Format;

use super::Target;

/// Version of the `pr no-effect` JSON contract.
///
/// Version 2 adds the prospective `merge_impact` analysis and the derived
/// `closure_recommendation`; the historical `effective_change`/`method` pair
/// keeps its version-1 meaning. The prefetch (#520) adds no field: it either
/// makes the existing verdict resolvable or explains itself through
/// `merge_impact.warnings`, which the contract already carries.
pub(super) const NO_EFFECT_SCHEMA_VERSION: u32 = 2;

#[derive(Debug, Serialize)]
pub(super) struct Report {
    pub(super) schema_version: u32,
    pub(super) mode: &'static str,
    pub(super) pr: Option<u64>,
    pub(super) base: String,
    pub(super) head: String,
    /// Historical branch-change verdict: merge-base..head tree delta.
    pub(super) effective_change: bool,
    /// Method behind `effective_change`: `git` or `compare-api`.
    pub(super) method: &'static str,
    /// Prospective merge impact against the current base.
    pub(super) merge_impact: MergeImpactReport,
    /// Whether the evidence supports suggesting closure. Never true for
    /// `UNKNOWN` or an unverified base.
    pub(super) closure_recommendation: bool,
}

/// What the post-fetch re-read says about the pair the proof planned to compare.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Snapshot {
    /// The base and head are exactly what the proof planned for.
    Unchanged,
    /// The base moved while the fetch was in flight.
    BaseMoved {
        /// The base the proof would have used.
        from: String,
        /// The base `GitHub` reports now.
        to: String,
    },
    /// The head moved, so a proof of the old head describes nothing current.
    HeadMoved {
        /// The head the proof planned to compare.
        from: String,
        /// The head `GitHub` reports now.
        to: String,
    },
}

/// Compares the view the proof planned against the view after the fetch.
///
/// The base is checked first: it is the side the issue scopes to, and a moved
/// base is the "behind" case. An unchanged view is the only outcome that lets a
/// proof be reported.
#[must_use]
pub(super) fn compare_snapshots(planned: &PrView, fresh: &PrView) -> Snapshot {
    if planned.base_ref_oid != fresh.base_ref_oid {
        return Snapshot::BaseMoved {
            from: planned.base_ref_oid.clone(),
            to: fresh.base_ref_oid.clone(),
        };
    }
    if planned.head_ref_oid != fresh.head_ref_oid {
        return Snapshot::HeadMoved {
            from: planned.head_ref_oid.clone(),
            to: fresh.head_ref_oid.clone(),
        };
    }
    Snapshot::Unchanged
}

/// Shortens an object id for a message, keeping it recognisable.
fn short(oid: &str) -> &str {
    oid.get(..8).unwrap_or(oid)
}

/// Runs `pr no-effect` and prints the report.
///
/// # Errors
///
/// Returns an error when the analysis cannot run at all. An `UNKNOWN` verdict,
/// including one caused by an unavailable prefetch, is a produced analysis and
/// therefore a success: it is the caller's decision that must fail closed, not
/// the report.
pub(super) fn run(root: &Path, target: Target, format: Format, no_fetch: bool) -> Result<()> {
    let report = analyze(root, target, no_fetch)?;
    if matches!(format, Format::Json) {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        print_text(&report);
    }
    Ok(())
}

fn print_text(report: &Report) {
    let prefix = report
        .pr
        .map_or_else(String::new, |number| format!("pr {number}: "));
    let base = report
        .merge_impact
        .base_sha
        .as_deref()
        .unwrap_or(&report.base);
    let head = report
        .merge_impact
        .head_sha
        .as_deref()
        .unwrap_or(&report.head);
    println!(
        "{prefix}merge-impact {} (base {base} head {head}, {}; branch-change {})",
        report.merge_impact.verdict,
        report.merge_impact.method,
        if report.effective_change {
            "has-effect"
        } else {
            "no-effect"
        }
    );
    if let Some(reason) = &report.merge_impact.reason {
        println!("  reason: {reason}");
    }
    if report.closure_recommendation {
        println!("  redundant: merging would not change the base tree");
    }
    for warning in &report.merge_impact.warnings {
        eprintln!("warning: {warning}");
    }
}

fn analyze(root: &Path, target: Target, no_fetch: bool) -> Result<Report> {
    match target {
        Target::Range { base, head } => Ok(analyze_range(root, base, head)?),
        Target::Pr(number) => Ok(analyze_pr(root, number, no_fetch)?),
    }
}

fn analyze_range(root: &Path, base: String, head: String) -> Result<Report> {
    let effect = no_effect::effective_change(root, &base, &head)
        .with_context(|| format!("cannot compare {base}...{head}"))?;
    let merge_impact = no_effect::merge_impact(root, &base, &head);
    let closure_recommendation = closure_recommendation(&merge_impact);
    Ok(Report {
        schema_version: NO_EFFECT_SCHEMA_VERSION,
        mode: "range",
        pr: None,
        base,
        head,
        effective_change: effect.has_effect(),
        method: "git",
        merge_impact,
        closure_recommendation,
    })
}

fn analyze_pr(root: &Path, number: u64, no_fetch: bool) -> Result<Report> {
    let planned = gh::view(root, number)?;
    let number = planned.number;
    let base = planned.base_ref_name.clone();
    let head = planned.head_ref_oid.clone();

    let mut warnings = Vec::new();
    let outcome = if no_fetch {
        // The operator asked for an untouched clone: the verdict is then
        // whatever the local objects support, exactly as before #520.
        PrefetchOutcome::Disabled
    } else {
        let timeout = fetch::fetch_timeout();
        fetch::prefetch(
            root,
            number,
            &planned.base_ref_oid,
            &planned.head_ref_oid,
            timeout,
        )
    };
    if let Some(warning) = outcome.warning() {
        warnings.push(warning);
    }
    if matches!(outcome, PrefetchOutcome::Fetched) {
        match compare_snapshots(&planned, &gh::view(root, number)?) {
            Snapshot::Unchanged => {}
            Snapshot::BaseMoved { from, to } => {
                return Ok(moved_report(
                    number,
                    &base,
                    &head,
                    warnings,
                    format!(
                        "the base moved while the proof was being prepared ({} -> {}); the \
                         branch is behind the current base, so no merge-impact verdict is \
                         proven — re-run once the base settles",
                        short(&from),
                        short(&to)
                    ),
                    "base-moved",
                ));
            }
            Snapshot::HeadMoved { from, to } => {
                return Ok(moved_report(
                    number,
                    &base,
                    &head,
                    warnings,
                    format!(
                        "the pull request head moved while the proof was being prepared \
                         ({} -> {}); a proof of the old head describes nothing current — \
                         re-run",
                        short(&from),
                        short(&to)
                    ),
                    "head-moved",
                ));
            }
        }
    }

    let mut merge_impact =
        no_effect::merge_impact_for_pr(root, &planned.base_ref_oid, &base, &head);
    let mut combined = warnings;
    combined.append(&mut merge_impact.warnings);
    merge_impact.warnings = combined;
    let closure_recommendation = closure_recommendation(&merge_impact);
    if let Some(effect) = local_effect(root, &base, &head) {
        return Ok(Report {
            schema_version: NO_EFFECT_SCHEMA_VERSION,
            mode: "pr",
            pr: Some(number),
            base,
            head,
            effective_change: effect.has_effect(),
            method: "git",
            merge_impact,
            closure_recommendation,
        });
    }
    let changed = gh::compare_file_count(root, &base, &head)
        .with_context(|| format!("cannot determine effective change for PR {number}"))?;
    Ok(Report {
        schema_version: NO_EFFECT_SCHEMA_VERSION,
        mode: "pr",
        pr: Some(number),
        base,
        head,
        effective_change: changed > 0,
        method: "compare-api",
        merge_impact,
        closure_recommendation,
    })
}

/// An `UNKNOWN` report for a pair that moved underneath the proof.
///
/// Closure is never recommended: the moved pair is exactly the case where a
/// stale proof would have been plausible.
fn moved_report(
    number: u64,
    base: &str,
    head: &str,
    warnings: Vec<String>,
    reason: String,
    evidence: &str,
) -> Report {
    let mut merge_impact = MergeImpactReport::unknown(base, head, reason);
    evidence.clone_into(&mut merge_impact.evidence);
    merge_impact.warnings = warnings;
    Report {
        schema_version: NO_EFFECT_SCHEMA_VERSION,
        mode: "pr",
        pr: Some(number),
        base: base.to_owned(),
        head: head.to_owned(),
        effective_change: false,
        method: "git",
        merge_impact,
        closure_recommendation: false,
    }
}

/// Closure is only suggested for a proven, base-verified no-tree-effect merge.
fn closure_recommendation(impact: &MergeImpactReport) -> bool {
    impact.verdict.is_no_tree_effect() && impact.base_verified
}

fn local_effect(root: &Path, base: &str, head: &str) -> Option<Effect> {
    if !no_effect::rev_exists(root, head) {
        return None;
    }
    for candidate in no_effect::base_refs(base) {
        if no_effect::rev_exists(root, &candidate) {
            if let Ok(effect) = no_effect::effective_change(root, &candidate, head) {
                return Some(effect);
            }
        }
    }
    None
}

#[cfg(test)]
#[path = "impact_tests.rs"]
mod tests;
