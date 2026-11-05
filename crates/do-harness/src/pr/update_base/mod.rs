//! `pr update-base`: guarded latest-base update and stack revalidation (#468).
//!
//! The command reports whether an open PR's head contains its current base
//! using the read-only ancestry proof from [`super::ancestry`]. On `--execute`
//! it updates an eligible own/known-bot head through `GitHub`'s `update-branch` API
//! pinned to the expected head (`--expected-head-sha`) so a concurrent author
//! push cannot be overwritten, or rebases a stacked child after its parent
//! merged. Every run is read-only unless `--execute` is passed and the global
//! `--dry-run` is not set.

mod eligibility;
mod execute;
mod gh;
mod stack;

use std::path::Path;

use anyhow::Result;
use serde::Serialize;

use self::gh::{current_user, open_prs, pr_author, repo_identity};
use super::ancestry::{self, AncestryDecision};
use crate::CliError;
use crate::pr::gh::{self as remote_gh, PrReadinessView};
use crate::report::Format;

/// Schema version of the `pr update-base` JSON report.
pub const UPDATE_BASE_SCHEMA_VERSION: u32 = 1;

/// Verdict of an update-base analysis.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    /// The head contains the base; nothing to update.
    UpToDate,
    /// The head does not contain the base; an update is required.
    UpdateRequired,
    /// A confirmed condition forbids the update.
    Blocked,
    /// The evidence could not be collected or proven.
    Unknown,
}

impl Verdict {
    /// Stable uppercase code used in the report.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::UpToDate => "UP_TO_DATE",
            Self::UpdateRequired => "UPDATE_REQUIRED",
            Self::Blocked => "BLOCKED",
            Self::Unknown => "UNKNOWN",
        }
    }
}

/// One reason in a report.
#[derive(Debug, Clone, Serialize)]
struct Reason {
    code: &'static str,
    message: String,
}

/// Eligibility state carried in the report.
#[derive(Debug, Clone, Serialize)]
struct EligibilityReport {
    eligible: bool,
    /// Stable code when the head is not eligible.
    reason: Option<&'static str>,
}

/// Stack state carried in the report.
#[derive(Debug, Clone, Serialize)]
struct StackReport {
    stacked: bool,
    /// An open PR still owns this PR's base branch.
    parent_open: bool,
}

/// Machine-readable output of `pr update-base`.
#[derive(Debug, Clone, Serialize)]
struct UpdateBaseReport {
    schema_version: u32,
    decision: &'static str,
    pr: u64,
    base_ref_name: String,
    head_ref_name: String,
    base_oid: String,
    head_oid: String,
    merge_base_oid: String,
    head_contains_base: bool,
    ancestry_decision: &'static str,
    merge_state_status: String,
    eligibility: EligibilityReport,
    executed: bool,
    new_head_oid: Option<String>,
    reasons: Vec<Reason>,
    stack: Option<StackReport>,
    /// Repository default branch (not serialized).
    #[serde(skip)]
    default_branch: String,
}

impl UpdateBaseReport {
    fn new(pr: u64) -> Self {
        Self {
            schema_version: UPDATE_BASE_SCHEMA_VERSION,
            decision: Verdict::Unknown.as_str(),
            pr,
            base_ref_name: String::new(),
            head_ref_name: String::new(),
            base_oid: String::new(),
            head_oid: String::new(),
            merge_base_oid: String::new(),
            head_contains_base: false,
            ancestry_decision: "unknown",
            merge_state_status: String::new(),
            eligibility: EligibilityReport {
                eligible: false,
                reason: None,
            },
            executed: false,
            new_head_oid: None,
            reasons: Vec::new(),
            stack: None,
            default_branch: String::new(),
        }
    }

    fn mark_unknown(&mut self, code: &'static str, message: impl Into<String>) {
        self.decision = Verdict::Unknown.as_str();
        self.reasons.push(Reason {
            code,
            message: message.into(),
        });
    }
}

/// Analyzes a PR and produces a report. Read-only.
fn analyze(root: &Path, pr: u64) -> UpdateBaseReport {
    let mut report = UpdateBaseReport::new(pr);

    let view: PrReadinessView = match remote_gh::readiness_view(root, pr) {
        Ok(view) => view,
        Err(err) => {
            report.mark_unknown("api_incomplete", format!("could not read PR {pr}: {err}"));
            return report;
        }
    };
    report.base_ref_name.clone_from(&view.base_ref_name);
    report.head_ref_name.clone_from(&view.head_ref_name);
    report.base_oid.clone_from(&view.base_ref_oid);
    report.head_oid.clone_from(&view.head_ref_oid);
    report
        .merge_state_status
        .clone_from(&view.merge_state_status);

    let repo = match repo_identity(root) {
        Ok(repo) => repo,
        Err(err) => {
            report.mark_unknown(
                "repo_unknown",
                format!("could not read repository identity: {err}"),
            );
            return report;
        }
    };
    if let Some(branch) = repo.default_branch_ref.as_ref() {
        report.default_branch.clone_from(&branch.name);
    }

    let proof = match ancestry::compare(root, &view.base_ref_oid, &view.head_ref_oid) {
        Ok(proof) => proof,
        Err(err) => {
            report.mark_unknown(
                "api_incomplete",
                format!("ancestry comparison failed: {err}"),
            );
            return report;
        }
    };
    report.merge_base_oid.clone_from(&proof.merge_base_sha);
    report.ancestry_decision = proof.decision.as_str();
    report.head_contains_base = proof.decision.is_ancestor();
    match proof.decision {
        AncestryDecision::Ahead | AncestryDecision::Identical => {
            report.decision = Verdict::UpToDate.as_str();
        }
        AncestryDecision::Diverged => {
            report.decision = Verdict::UpdateRequired.as_str();
        }
        AncestryDecision::Unknown => {
            report.mark_unknown("ancestry_unknown", proof.reason.clone().unwrap_or_default());
            return report;
        }
    }

    let current_user = current_user(root).unwrap_or_default();
    let author = pr_author(root, pr).unwrap_or_default();
    report.eligibility =
        match eligibility::evaluate(root, &view, &repo.name_with_owner, &current_user, &author) {
            eligibility::EligibilityOutcome::Eligible => EligibilityReport {
                eligible: true,
                reason: None,
            },
            eligibility::EligibilityOutcome::Blocked(code)
            | eligibility::EligibilityOutcome::Unknown(code) => EligibilityReport {
                eligible: false,
                reason: Some(code),
            },
        };

    if view.base_ref_name != report.default_branch {
        let parent_open = open_prs(root).is_ok_and(|prs| {
            prs.iter()
                .any(|candidate| candidate.head_ref_name == view.base_ref_name)
        });
        report.stack = Some(StackReport {
            stacked: true,
            parent_open,
        });
    }

    report
}

/// Applies `--execute`: mutates `report` when an update is performed.
#[allow(clippy::too_many_lines)]
fn execute_analysis(root: &Path, report: &mut UpdateBaseReport) {
    if report.decision != Verdict::UpdateRequired.as_str() {
        return;
    }
    if !report.eligibility.eligible {
        report.decision = Verdict::Blocked.as_str();
        let code = report.eligibility.reason.unwrap_or("unauthorized");
        report.reasons.push(Reason {
            code,
            message: "head is not an authorized own/known-bot branch".to_owned(),
        });
        return;
    }

    if let Some(stacked) = &report.stack {
        if stacked.parent_open {
            report.decision = Verdict::Blocked.as_str();
            report.reasons.push(Reason {
                code: "stack_parent_first",
                message: "an open parent PR owns the base; merge it first".to_owned(),
            });
            return;
        }
        match stack::rebase(
            root,
            report.pr,
            &report.head_ref_name,
            &report.base_ref_name,
            &report.default_branch,
        ) {
            Ok(stack::StackOutcome::Rebased { new_head }) => {
                report.executed = true;
                report.new_head_oid = Some(new_head);
                report.reasons.push(Reason {
                    code: "revalidate_required",
                    message: "rebased the child onto the new base; rerun every gate from scratch"
                        .to_owned(),
                });
            }
            Ok(stack::StackOutcome::ParentFirst) => {
                report.decision = Verdict::Blocked.as_str();
                report.reasons.push(Reason {
                    code: "stack_parent_first",
                    message: "an open parent PR owns the base; merge it first".to_owned(),
                });
            }
            Ok(stack::StackOutcome::ParentGone) => {
                report.decision = Verdict::Blocked.as_str();
                report.reasons.push(Reason {
                    code: "stack_parent_gone",
                    message: "the parent branch reference is gone; recover its old head to rebase"
                        .to_owned(),
                });
            }
            Ok(stack::StackOutcome::Conflict) => {
                report.decision = Verdict::Blocked.as_str();
                report.reasons.push(Reason {
                    code: "stack_conflict",
                    message: "rebase conflicted or would duplicate parent changes; nothing changed"
                        .to_owned(),
                });
            }
            Err(err) => {
                report.mark_unknown("execution_error", format!("stacked rebase failed: {err}"));
            }
        }
        return;
    }

    let outcome = execute::update(root, report.pr, &report.head_oid, &report.base_oid);
    let outcome = match outcome {
        Ok(outcome) => outcome,
        Err(err) => {
            report.mark_unknown("execution_error", format!("branch update failed: {err}"));
            return;
        }
    };
    match outcome {
        execute::ExecuteOutcome::Updated { head, base } => {
            report.executed = true;
            report.new_head_oid = Some(head.clone());
            report.reasons.push(Reason {
                code: "revalidate_required",
                message: format!("updated head to {head}; rerun every gate from scratch"),
            });
            if let Ok(refreshed) = ancestry::compare(root, &base, &head) {
                report.merge_base_oid.clone_from(&refreshed.merge_base_sha);
                report.ancestry_decision = refreshed.decision.as_str();
                report.head_contains_base = refreshed.decision.is_ancestor();
                if !refreshed.decision.is_ancestor() {
                    report.mark_unknown(
                        "update_unverified",
                        "updated head does not contain the base",
                    );
                }
            }
        }
        execute::ExecuteOutcome::Rejected403 => {
            report.decision = Verdict::Blocked.as_str();
            report.reasons.push(Reason {
                code: "update_rejected_403",
                message: "GitHub denied the branch update (403); no retry attempted".to_owned(),
            });
        }
        execute::ExecuteOutcome::Rejected422 => {
            report.decision = Verdict::Blocked.as_str();
            report.reasons.push(Reason {
                code: "update_rejected_422",
                message: "GitHub rejected the expected-head update (422); author work was not overwritten".to_owned(),
            });
        }
        execute::ExecuteOutcome::BaseMoved { base } => {
            report.mark_unknown(
                "base_moved",
                format!("the base moved to {base} during the update"),
            );
        }
        execute::ExecuteOutcome::Timeout => {
            report.mark_unknown("poll_timeout", "no new head appeared before the deadline");
        }
    }
}

fn render_text(report: &UpdateBaseReport) {
    println!("pr {}: {}", report.pr, report.decision);
    println!("  base {} @ {}", report.base_ref_name, report.base_oid);
    println!("  head {} @ {}", report.head_ref_name, report.head_oid);
    println!(
        "  merge-base {} (head contains base: {})",
        report.merge_base_oid, report.head_contains_base
    );
    println!(
        "  ancestry: {} (merge state {})",
        report.ancestry_decision, report.merge_state_status
    );
    if report.eligibility.eligible {
        println!("  eligibility: eligible");
    } else {
        println!(
            "  eligibility: not eligible ({})",
            report.eligibility.reason.unwrap_or("unknown")
        );
    }
    if report.executed {
        let new = report.new_head_oid.as_deref().unwrap_or("?");
        println!("  executed: updated to {new}");
    }
    for reason in &report.reasons {
        println!("  [{}] {}", reason.code, reason.message);
    }
}

/// Runs `pr update-base PR [--dry-run|--execute]`, printing in `format`.
#[allow(clippy::too_many_lines)]
pub fn run(
    root: &Path,
    pr: u64,
    execute: bool,
    dry_run_global: bool,
    format: Format,
) -> Result<(), CliError> {
    let mut report = analyze(root, pr);
    if execute && !dry_run_global {
        execute_analysis(root, &mut report);
    }
    if matches!(format, Format::Json) {
        let json = serde_json::to_string_pretty(&report)
            .map_err(|err| CliError::Verify(anyhow::anyhow!("cannot serialize report: {err}")))?;
        println!("{json}");
    } else {
        render_text(&report);
    }
    match report.decision {
        "UP_TO_DATE" | "UPDATE_REQUIRED" => Ok(()),
        "BLOCKED" => {
            let reason = report.reasons.first().map_or("blocked", |r| r.code);
            Err(CliError::Verify(anyhow::anyhow!(
                "pr {} update-base blocked: {reason}",
                report.pr
            )))
        }
        _ => {
            let reason = report.reasons.first().map_or("unknown", |r| r.code);
            Err(CliError::Usage(anyhow::anyhow!(
                "pr {} update-base is unknown: {reason}",
                report.pr
            )))
        }
    }
}
