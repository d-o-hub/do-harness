//! Sweep report schema (version 1) and rendering.
//!
//! The report is the only structured output; every per-PR outcome is a row,
//! including drafts, holds, external authors, and halted PRs. The deterministic
//! path never calls a model, so `zero_model` is always true.

use serde::Serialize;

use crate::pr::readiness::AuthorizationReason;
use crate::pr::readiness::{CheckSummary, ConversationSummary, FindingsSummary};
use crate::report::Format;

use super::state::ModeRecord;

/// Branch-recovery mechanism recorded on a sweep row (#518).
///
/// The variants are the ordered recovery chain: the API update, then the
/// documented dependency-bot rebase command, then nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum BranchUpdate {
    /// `PUT /pulls/{n}/update-branch` was accepted and moved the head.
    UpdateBranch,
    /// The documented `@dependabot rebase` command was posted.
    DependabotRebase,
    /// Nothing moved the branch; a human must.
    None,
}

/// Report schema version.
pub const SWEEP_REPORT_SCHEMA_VERSION: u32 = 1;

/// Target selector as executed.
#[derive(Debug, Clone, Serialize)]
pub struct TargetRecord {
    /// Whether the whole open inventory was targeted.
    pub all: bool,
    /// Single-PR target, when used.
    pub pr: Option<u64>,
}

/// Why the whole sweep stopped.
#[derive(Debug, Clone, Serialize)]
pub struct HaltRecord {
    /// `regression` or `unknown`.
    pub code: String,
    /// Human-readable detail.
    pub message: String,
    /// The PR after whose merge the halt was decided.
    pub pr: u64,
}

/// Duplicate-cluster role for one PR.
#[derive(Debug, Clone, Serialize)]
pub struct ClusterRecord {
    /// `keeper` or `loser`.
    pub role: String,
    /// Keeper number (set for losers).
    pub keeper: Option<u64>,
    /// Keeper rule: `a`, `d`, or `tie`.
    pub rule: Option<String>,
}

/// One PR row.
#[derive(Debug, Clone, Serialize)]
pub struct PerPrReport {
    /// Pull request number.
    pub number: u64,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub title: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub url: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub author: String,
    /// Whether the author is sweep-owned (own/bot/admin).
    pub authorized: bool,
    /// Terminal outcome: `skipped`, `blocked`, `unknown`, `deferred`,
    /// `closed_no_effect`, `closed_superseded`, or `merged`.
    pub outcome: String,
    /// Stable reason codes (reusing readiness authorization codes where they
    /// apply).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reasons: Vec<AuthorizationReason>,
    /// Human-readable blocker lines.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub blockers: Vec<String>,
    /// Explicit remediation command/action, when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_action: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_sha: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub head_sha: Option<String>,
    /// Which branch-recovery mechanism was used for a `BEHIND` base (#518).
    ///
    /// Absent when the branch was never behind. Present on every row that ran
    /// the chain, so "the fallback was not used opportunistically" is readable
    /// from the report rather than inferred from the mutation log.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub branch_update: Option<BranchUpdate>,
    /// Classified checks from the final snapshot, when collected.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub checks: Option<CheckSummary>,
    /// Conversation ledger from the final snapshot, when collected.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub conversations: Option<ConversationSummary>,
    /// Findings summary from the final snapshot, when collected.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub findings: Option<FindingsSummary>,
    /// Merge commit SHA after a direct merge.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub merge_sha: Option<String>,
    /// Post-merge main verification: `pass`, `regression`, or `unknown`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub post_merge: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cluster: Option<ClusterRecord>,
}

/// Aggregate counts.
#[derive(Debug, Clone, Default, Serialize)]
pub struct SummaryRecord {
    pub skipped: usize,
    pub blocked: usize,
    pub unknown: usize,
    pub deferred: usize,
    pub merged: usize,
    pub closed_no_effect: usize,
    pub closed_superseded: usize,
}

/// The schema-1 sweep report.
#[derive(Debug, Clone, Serialize)]
pub struct SweepReport {
    pub schema_version: u32,
    pub mode: ModeRecord,
    pub target: TargetRecord,
    /// Wait bound used for CI and post-merge verification.
    pub wait_seconds: u64,
    /// Always true: the deterministic path never calls a model.
    pub zero_model: bool,
    /// Where the audit state is stored.
    pub state_path: String,
    pub prs: Vec<PerPrReport>,
    pub summary: SummaryRecord,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub halted: Option<HaltRecord>,
}

impl BranchUpdate {
    /// The stable token recorded in the report and read by tests.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::UpdateBranch => "update-branch",
            Self::DependabotRebase => "dependabot-rebase",
            Self::None => "none",
        }
    }
}

impl SweepReport {
    /// Prints the report in `format`.
    pub fn render(&self, format: Format) {
        match format {
            Format::Json => match serde_json::to_string_pretty(self) {
                Ok(serialized) => println!("{serialized}"),
                Err(error) => eprintln!("warning: failed to serialize sweep report: {error}"),
            },
            Format::Text => self.render_text(),
        }
    }

    fn render_text(&self) {
        let mode = if self.mode.dry_run {
            "dry-run".to_owned()
        } else if self.mode.merge {
            "execute --merge".to_owned()
        } else {
            "execute".to_owned()
        };
        let target = match self.target.pr {
            Some(number) => format!("pr {number}"),
            None => "all open PRs".to_owned(),
        };
        println!(
            "sweep: {target} ({mode}, zero-model, state {})",
            self.state_path
        );
        for pr in &self.prs {
            println!(
                "pr {}: {} #{} (head {} base {})",
                pr.number,
                pr.outcome,
                pr.title,
                pr.head_sha.as_deref().unwrap_or("-"),
                pr.base_sha.as_deref().unwrap_or("-"),
            );
            for reason in &pr.reasons {
                println!("  {}: {}", reason.code, reason.message);
            }
            for blocker in &pr.blockers {
                println!("  {blocker}");
            }
            if let Some(update) = pr.branch_update {
                println!("  branch update: {}", update.as_str());
            }
            if let Some(action) = &pr.next_action {
                println!("  next: {action}");
            }
            if let Some(sha) = &pr.merge_sha {
                println!("  merge sha {sha}");
                if let Some(status) = &pr.post_merge {
                    println!("  post-merge main: {status}");
                }
            }
        }
        let s = &self.summary;
        println!(
            "summary: merged {} closed-no-effect {} closed-superseded {} blocked {} unknown {} deferred {} skipped {}",
            s.merged,
            s.closed_no_effect,
            s.closed_superseded,
            s.blocked,
            s.unknown,
            s.deferred,
            s.skipped
        );
        if let Some(halt) = &self.halted {
            println!(
                "halted after pr {}: {} — {}",
                halt.pr, halt.code, halt.message
            );
        }
    }
}
