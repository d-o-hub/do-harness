//! `do-harness design-review` subcommand arguments.
//!
//! Split from `actions.rs` to keep that file under the modularity cap.

use std::path::PathBuf;

use clap::{ArgAction, Args, Subcommand, ValueHint};

use crate::design_review::bridge::DEFAULT_MAX_WAIT_S;

/// Disposition verdicts for a recorded finding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum DispositionStatus {
    /// The finding is real; apply the smallest correction through the
    /// normal implementation and verification workflow.
    Confirmed,
    /// The finding does not apply; the note carries the evidence.
    Rejected,
    /// Remains an explicit, recorded decision.
    Unresolved,
}

impl DispositionStatus {
    /// Wire name recorded in the record's dispositions array.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Confirmed => "confirmed",
            Self::Rejected => "rejected",
            Self::Unresolved => "unresolved",
        }
    }
}

/// Arguments for `do-harness design-review run`.
#[derive(Debug, Args)]
pub struct DesignReviewRunArgs {
    /// Frozen design document (repository-relative path).
    #[arg(long, value_hint = ValueHint::FilePath, value_name = "FILE")]
    pub design_doc: PathBuf,
    /// Revision every packet path is pinned to.
    #[arg(long, value_name = "REV", default_value = "HEAD")]
    pub revision: String,
    /// Write the sanitized record here (canonical JSON).
    #[arg(long, value_hint = ValueHint::FilePath, value_name = "FILE")]
    pub record_out: PathBuf,
    /// Opaque review identifier (default: record-out file stem).
    #[arg(long, value_name = "ID")]
    pub review_id: Option<String>,
    /// Write the assembled packet here (canonical JSON).
    #[arg(long, value_hint = ValueHint::FilePath, value_name = "FILE")]
    pub packet_out: Option<PathBuf>,
    /// The concrete design risk in one or two sentences.
    #[arg(long, value_name = "TEXT")]
    pub objective: String,
    /// Expected observable behavior.
    #[arg(long, value_name = "TEXT")]
    pub intended_behavior: Option<String>,
    /// Explicit constraint (repeatable).
    #[arg(long, value_name = "TEXT", action = ArgAction::Append)]
    pub constraint: Vec<String>,
    /// The candidate contract (signatures, schemas, semantics).
    #[arg(long, value_name = "TEXT")]
    pub proposed_contract: String,
    /// Open question the author has genuinely not decided (repeatable).
    #[arg(long, value_name = "TEXT", action = ArgAction::Append)]
    pub open_question: Vec<String>,
    /// Repository-relative source path (repeatable).
    #[arg(long, value_name = "PATH", action = ArgAction::Append)]
    pub source_path: Vec<String>,
    /// Repository-relative test path (repeatable).
    #[arg(long, value_name = "PATH", action = ArgAction::Append)]
    pub test_path: Vec<String>,
    /// Repository-relative document path (repeatable).
    #[arg(long, value_name = "PATH", action = ArgAction::Append)]
    pub doc_path: Vec<String>,
    /// Reviewer process argv (JSON string array) that receives the packet on
    /// stdin. Explicit opt-in: without it nothing launches and the record
    /// says `not_run`.
    #[arg(long, value_name = "JSON")]
    pub reviewer_command_json: Option<String>,
    /// Parent-only isolation marker probed in the captured reviewer output;
    /// a leaked marker fails the review.
    #[arg(long, value_name = "MARKER")]
    pub parent_marker: Option<String>,
    /// Reviewer deadline in seconds (default 300).
    #[arg(long, value_name = "SECS")]
    pub limit_timeout: Option<u64>,
    /// Material-findings cap (default 5).
    #[arg(long, value_name = "N")]
    pub limit_findings: Option<usize>,
    /// Reviewer output cap in bytes (default 65536).
    #[arg(long, value_name = "BYTES")]
    pub limit_output: Option<usize>,
    /// Host/client/model provenance recorded with the record (JSON object).
    #[arg(long, value_name = "JSON", default_value = "{}")]
    pub meta: String,
}

/// Available `design-review` actions.
#[derive(Debug, Subcommand)]
pub enum DesignReviewAction {
    /// Assemble a frozen packet from a design document, launch one fresh
    /// reviewer process, and record the sanitized outcome.
    Run(Box<DesignReviewRunArgs>),
    /// Disposition a recorded finding as confirmed, rejected, or unresolved.
    Dispose {
        /// Record file to update.
        #[arg(long, value_hint = ValueHint::FilePath, value_name = "FILE")]
        record: PathBuf,
        /// Finding id to disposition.
        #[arg(long, value_name = "ID")]
        finding_id: String,
        /// Disposition verdict.
        #[arg(long, value_enum)]
        status: DispositionStatus,
        /// Evidence-backed note recorded with the disposition.
        #[arg(long, value_name = "TEXT")]
        note: Option<String>,
    },
    /// Act as a host file-handoff reviewer bridge: packet on stdin,
    /// findings on stdout. Exit 9 with `capability unavailable` on stderr
    /// when no findings file appears within the wait.
    Bridge {
        /// Handoff directory for `packet.json` and `findings.json`.
        #[arg(long, value_hint = ValueHint::DirPath, value_name = "DIR")]
        handoff_dir: PathBuf,
        /// Seconds to wait for the findings file before reporting the
        /// capability unavailable.
        #[arg(long, value_name = "SECS", default_value_t = DEFAULT_MAX_WAIT_S)]
        max_wait_s: f64,
    },
}
