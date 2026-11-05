//! PR subcommand action definitions.

use std::path::PathBuf;

use clap::{Subcommand, ValueHint};

use crate::report::Format;

/// Available deterministic PR analysis actions.
#[derive(Debug, Subcommand)]
pub enum PrAction {
    /// Report the prospective merge impact of a PR or revision range.
    NoEffect {
        /// Pull request number; resolves base and head through `gh`.
        #[arg(value_name = "PR", conflicts_with_all = ["base", "head"])]
        pr: Option<u64>,
        /// Base revision (local mode; requires --head).
        #[arg(long, value_name = "REV", requires = "head")]
        base: Option<String>,
        /// Head revision (local mode; requires --base).
        #[arg(long, value_name = "REV", requires = "base")]
        head: Option<String>,
        /// Output format.
        #[arg(long, value_enum, default_value_t = Format::Text)]
        format: Format,
    },
    /// Emit the semantic residual: changed units evidence could not prove.
    Review {
        /// Pull request number; resolves base and head through `gh`.
        #[arg(value_name = "PR", conflicts_with_all = ["base", "head"])]
        pr: Option<u64>,
        /// Base revision (local mode; requires --head).
        #[arg(long, value_name = "REV", requires = "head")]
        base: Option<String>,
        /// Head revision (local mode; requires --base).
        #[arg(long, value_name = "REV", requires = "base")]
        head: Option<String>,
        /// Ignore the cached report and recompute from scratch.
        #[arg(long)]
        recompute: bool,
        /// Review payload view.
        #[arg(long, value_enum, default_value_t = crate::pr::ReviewView::Audit)]
        view: crate::pr::ReviewView,
        /// Output format.
        #[arg(long, value_enum, default_value_t = Format::Text)]
        format: Format,
    },
    /// Check whether a pull request is ready to merge: merge state, CI checks, and review conversations.
    #[command(visible_alias = "readiness", visible_alias = "check")]
    Ready {
        /// Pull request number.
        #[arg(value_name = "PR")]
        pr: u64,
        /// Re-validate a previously issued authorization receipt (the full
        /// `pr ready --format json` report file). The snapshot is recomputed.
        #[arg(long, value_hint = ValueHint::FilePath, value_name = "FILE")]
        expected_receipt: Option<PathBuf>,
        /// Output format.
        #[arg(long, value_enum, default_value_t = Format::Text)]
        format: Format,
    },
    /// Classify a pull request's CI checks and commit statuses; fail closed on unknown.
    Checks {
        /// Pull request number.
        #[arg(value_name = "PR")]
        pr: u64,
        /// Output format.
        #[arg(long, value_enum, default_value_t = Format::Text)]
        format: Format,
    },
    /// Classify patch-coverage residue: waivable classes and lines needing tests.
    Waivers {
        /// Pull request number; resolves base and head through `gh`.
        #[arg(value_name = "PR", conflicts_with_all = ["base", "head", "patch"])]
        pr: Option<u64>,
        /// Base revision (local mode; requires --head).
        #[arg(long, value_name = "REV", requires = "head", conflicts_with = "patch")]
        base: Option<String>,
        /// Head revision (local mode; requires --base).
        #[arg(long, value_name = "REV", requires = "base", conflicts_with = "patch")]
        head: Option<String>,
        /// Patch file (unified diff) instead of a revision range.
        #[arg(long, value_hint = ValueHint::FilePath, value_name = "PATH")]
        patch: Option<PathBuf>,
        /// Measured lcov report to classify.
        #[arg(long, value_hint = ValueHint::FilePath, value_name = "PATH")]
        lcov: PathBuf,
        /// Previous lcov report: also report the changed lines covered since.
        #[arg(long, value_hint = ValueHint::FilePath, value_name = "PATH")]
        since: Option<PathBuf>,
        /// Prefix stripped from `SF:` paths (a CI checkout path).
        #[arg(long, value_name = "PREFIX")]
        strip_prefix: Option<String>,
        /// Output format; `text` prints the paste-ready review comment.
        #[arg(long, value_enum, default_value_t = Format::Text)]
        format: Format,
    },
    /// Record a structured disposition for a review, thread, comment, or finding.
    Disposition {
        /// Pull request number.
        #[arg(value_name = "PR")]
        pr: u64,
        /// Item reference (e.g. `issue-comment:123`, `review-thread:PRRT_...`, `review:PRR_...`, `finding:...`).
        #[arg(value_name = "ITEM_REF")]
        item_ref: String,
        /// Decision for the item (fixed, non-actionable, or waived).
        #[arg(long, value_enum)]
        decision: crate::pr::conversations::DispositionDecision,
        /// Full head commit SHA to bind the disposition to.
        #[arg(long)]
        head: String,
        /// Concrete rationale for the decision.
        #[arg(long)]
        reason: String,
        /// Optional evidence SHA (required if decision is fixed).
        #[arg(long)]
        evidence: Option<String>,
        /// Output format.
        #[arg(long, value_enum, default_value_t = Format::Text)]
        format: Format,
    },
    /// Collect and classify findings (annotations, code scanning, artifacts) and track issues.
    Findings {
        /// Pull request number.
        #[arg(value_name = "PR")]
        pr: u64,
        /// Issue creation mode: `plan` (preview only) or `create`.
        #[arg(long, value_enum, default_value_t = crate::pr::findings::IssuesMode::Plan)]
        issues: crate::pr::findings::IssuesMode,
        /// Output format.
        #[arg(long, value_enum, default_value_t = Format::Json)]
        format: Format,
    },
    /// Update a pull request's branch to the latest base without overwriting
    /// author work (default is read-only; `--execute` acts only on an
    /// authorized own/known-bot head).
    UpdateBase {
        /// Pull request number.
        #[arg(value_name = "PR")]
        pr: u64,
        /// Execute the update (default is a read-only report).
        #[arg(long)]
        execute: bool,
        /// Output format.
        #[arg(long, value_enum, default_value_t = Format::Text)]
        format: Format,
    },
    /// Direct PR number invocation (e.g. `do-harness pr 240`).
    #[command(external_subcommand)]
    External(Vec<String>),
}
