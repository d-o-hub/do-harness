//! The cleanup report: one schema-versioned document, rendered as text or JSON.

use serde::Serialize;

use super::Reason;
use super::proof::MergeProof;

/// Report schema version.
pub const SCHEMA_VERSION: u32 = 1;

/// The verdict a candidate received.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Verdict {
    /// Provably disposable; not reclaimed because this was a dry run or `--keep` held it.
    Eligible,
    /// Reclaimed in this run.
    Cleaned,
    /// Refused for the recorded reason.
    Refused,
    /// Eligible, but the deletion failed.
    Failed,
}

impl Verdict {
    /// Wire spelling.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Eligible => "eligible",
            Self::Cleaned => "cleaned",
            Self::Refused => "refused",
            Self::Failed => "failed",
        }
    }
}

/// One worktree considered by this run.
#[derive(Debug, Clone, Serialize)]
pub struct Candidate {
    /// Absolute path of the worktree.
    pub worktree: String,
    /// Short branch name, absent for a detached head.
    pub branch: Option<String>,
    /// Commit the worktree's `HEAD` names.
    pub head: String,
    /// Build tree considered for reclamation.
    pub target_dir: Option<String>,
    /// Size of the build tree when it could be measured.
    pub target_bytes: Option<u64>,
    /// Liveness probes that ran.
    pub probe: Option<&'static str>,
    /// Merge proof recorded for this candidate.
    pub merge_proof: MergeProof,
    /// What happened.
    pub verdict: Verdict,
    /// Why the candidate was refused, when it was.
    pub reason: Option<&'static str>,
    /// Bytes reclaimed, or `null` on a dry run.
    pub reclaimed_bytes: Option<u64>,
    /// Time this candidate took, in milliseconds.
    pub duration_ms: u64,
}

/// The whole run.
#[derive(Debug, Clone, Serialize)]
pub struct Report {
    /// Schema version, so a field can be added without breaking a consumer.
    pub schema_version: u32,
    /// Repository root the run inspected.
    pub root: String,
    /// When the run was generated.
    pub generated_at_unix: i64,
    /// Whether anything was reclaimed.
    pub dry_run: bool,
    /// Commit `origin/main` named at the start of the run.
    pub origin_main: Option<String>,
    /// When that remote-tracking ref last moved, when knowable.
    pub origin_main_fetched_at_unix: Option<i64>,
    /// Every candidate, in discovery order.
    pub candidates: Vec<Candidate>,
    /// Total bytes reclaimed by this run.
    pub reclaimed_bytes: u64,
}

impl Report {
    /// Whether any candidate carries an error-class refusal or a failed deletion.
    #[must_use]
    pub fn has_errors(&self) -> bool {
        self.candidates.iter().any(|candidate| {
            candidate.verdict == Verdict::Failed
                || candidate
                    .reason
                    .and_then(Reason::from_code)
                    .is_some_and(Reason::is_error_class)
        })
    }

    /// Human-readable rendering.
    #[must_use]
    pub fn render_text(&self) -> String {
        use std::fmt::Write as _;
        let mut out = String::new();
        let _ = writeln!(
            out,
            "cleanup{}: {} candidate(s) under {}",
            if self.dry_run { " (dry run)" } else { "" },
            self.candidates.len(),
            self.root
        );
        for candidate in &self.candidates {
            let branch = candidate.branch.as_deref().unwrap_or("(detached)");
            let bytes = candidate
                .target_bytes
                .map_or_else(|| "unknown size".to_owned(), human_bytes);
            let _ = writeln!(
                out,
                "  {:<8} {} [{}] {} - {}",
                candidate.verdict.as_str(),
                candidate.worktree,
                branch,
                bytes,
                describe(candidate)
            );
        }
        let _ = writeln!(
            out,
            "reclaimed {}{}",
            human_bytes(self.reclaimed_bytes),
            if self.dry_run {
                " (nothing written)"
            } else {
                ""
            }
        );
        out
    }
}

/// One-line explanation of a candidate's outcome.
fn describe(candidate: &Candidate) -> String {
    if let Some(reason) = candidate.reason {
        return reason.to_owned();
    }
    match candidate.merge_proof.kind {
        super::proof::ProofKind::PrMerged => format!(
            "merged in #{}",
            candidate
                .merge_proof
                .pr
                .map_or_else(|| "?".to_owned(), |pr| pr.to_string())
        ),
        super::proof::ProofKind::Ancestor => "merged (ancestor)".to_owned(),
        super::proof::ProofKind::TreeAbsorbed => "merged (tree absorbed)".to_owned(),
        super::proof::ProofKind::Unproven => "no proof".to_owned(),
    }
}

/// Formats a byte count with binary units.
#[must_use]
#[allow(clippy::cast_precision_loss)] // display only: the mantissa is irrelevant at GiB scale
pub fn human_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit + 1 < UNITS.len() {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}
