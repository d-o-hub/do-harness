//! Compact, versioned agent-facing PR review envelope (WF-03).
//!
//! Exposes review-required units and audit-only exemption metadata while
//! selecting the smaller complete representation between structured residual
//! units and raw diff.

use std::fmt::Write as _;

use serde::{Deserialize, Serialize};

use super::diff::{Change, Unit};
use super::review::{FalseProven, ReviewReport};

/// Representation format for review-required content.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Representation {
    /// Structured residual unit array.
    Residual,
    /// Complete unified diff of review-required units.
    RawDiff,
}

/// Selected view of the PR review payload.
#[derive(clap::ValueEnum, Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ReviewView {
    /// Full audit report including exempt and proven changes.
    #[default]
    Audit,
    /// Compact agent-facing envelope with review-required content.
    Agent,
}

impl std::fmt::Display for ReviewView {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Audit => write!(f, "audit"),
            Self::Agent => write!(f, "agent"),
        }
    }
}

/// Lightweight reference to an exempt or proven unit without payload lines.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditUnitRef {
    /// Unit identity (`path:new_start`).
    pub id: String,
    /// Repository-relative path.
    pub path: String,
    /// File-level change kind.
    pub change: Change,
    /// Header/anchor, when present.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub header: Option<String>,
}

/// Semantic marker for binary, rename, mode, or parse-uncertain changes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewMarker {
    /// Associated unit ID.
    pub id: String,
    /// Marker classification.
    pub kind: String,
}

/// Byte and token measurement for the emitted envelope.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EnvelopeMeasurement {
    /// UTF-8 byte length of this emitted envelope.
    pub emitted_bytes: usize,
    /// UTF-8 byte length of the equivalent audit report.
    pub audit_bytes: usize,
    /// Ratio of emitted bytes to audit bytes.
    pub ratio: f64,
    /// Measured token cost, if supplied by a trusted adapter.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token_cost: Option<u64>,
    /// Currency cost, if supplied by a trusted adapter.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub currency_cost: Option<f64>,
    /// Why token/currency costs are unmeasured.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage_reason: Option<String>,
}

/// Compact agent-facing PR review envelope.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentReviewEnvelope {
    /// Envelope schema version.
    pub schema_version: u32,
    /// Review mode (`range` or `pr`).
    pub mode: String,
    /// Pull request number, when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pr: Option<u64>,
    /// Base revision.
    pub base: String,
    /// Head revision.
    pub head: String,
    /// Resolved merge base.
    pub merge_base: String,
    /// Merge-base policy digest or "none".
    pub policy_digest: String,
    /// Chosen content representation.
    pub representation: Representation,
    /// List of review-required unit IDs.
    pub review_required_units: Vec<String>,
    /// Special change markers (binary, rename-only, mode-change, parse-uncertain).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub markers: Vec<ReviewMarker>,
    /// Structured review-required units (present when representation is residual).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub units: Option<Vec<Unit>>,
    /// Unified diff string (present when representation is `raw_diff`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raw_diff: Option<String>,
    /// Number of policy-exempt units.
    pub exempt_count: usize,
    /// Exemption identities for audit.
    pub exempt_units: Vec<AuditUnitRef>,
    /// Number of evidence-proven units.
    pub skipped_count: usize,
    /// Proven unit identities for audit.
    pub skipped_units: Vec<AuditUnitRef>,
    /// Revoked mechanical claims.
    pub false_proven: Vec<FalseProven>,
    /// Non-fatal diagnostics.
    pub warnings: Vec<String>,
    /// Emitted byte and cost accounting.
    pub measurement: EnvelopeMeasurement,
}

fn to_ref(unit: &Unit) -> AuditUnitRef {
    AuditUnitRef {
        id: unit.id.clone(),
        path: unit.path.clone(),
        change: unit.change,
        header: unit.header.clone(),
    }
}

fn unit_marker(unit: &Unit) -> Option<ReviewMarker> {
    let kind = match unit.header.as_deref() {
        Some("binary") => "binary",
        Some(super::diff::HEADER_RENAME_ONLY) => "rename-only",
        Some(super::diff::HEADER_MODE_CHANGE) => "mode-change",
        _ if unit.old_start == 0 && unit.new_start == 0 && unit.lines.is_empty() => {
            "parse-uncertain"
        }
        _ => return None,
    };
    Some(ReviewMarker {
        id: unit.id.clone(),
        kind: kind.to_owned(),
    })
}

fn render_raw_diff(units: &[Unit]) -> String {
    let mut out = String::new();
    for unit in units {
        let old_p = unit.old_path.as_deref().unwrap_or(&unit.path);
        let _ = writeln!(out, "--- a/{old_p}");
        let _ = writeln!(out, "+++ b/{}", unit.path);
        let hdr = unit
            .header
            .as_deref()
            .map(|h| format!(" {h}"))
            .unwrap_or_default();
        let _ = writeln!(out, "@@ -{} +{} @@{}", unit.old_start, unit.new_start, hdr);
        for line in &unit.lines {
            out.push_str(line);
            out.push('\n');
        }
    }
    out
}

const RATIO_SCALE: f64 = 1000.0;

#[allow(clippy::cast_precision_loss)]
fn calc_ratio(emitted: usize, audit: usize) -> f64 {
    if audit == 0 {
        0.0
    } else {
        ((emitted as f64 / audit as f64) * RATIO_SCALE).round() / RATIO_SCALE
    }
}
/// Builds the compact agent-facing envelope from an evaluated review report,
/// choosing the smaller complete representation between residual and raw diff.
#[must_use]
pub fn build_agent_envelope(report: &ReviewReport) -> AgentReviewEnvelope {
    let audit_bytes = serde_json::to_string_pretty(report).map_or(0, |s| s.len());
    let policy_digest = report.policy.sha256.clone();
    let review_required_units: Vec<String> = report.residual.iter().map(|u| u.id.clone()).collect();
    let markers: Vec<ReviewMarker> = report.residual.iter().filter_map(unit_marker).collect();
    let exempt_units: Vec<AuditUnitRef> = report.exempt.iter().map(to_ref).collect();
    let skipped_units: Vec<AuditUnitRef> = report.skipped.iter().map(to_ref).collect();
    let raw_diff_str = render_raw_diff(&report.residual);

    let create_candidate = |rep: Representation,
                            units: Option<Vec<Unit>>,
                            raw_diff: Option<String>|
     -> AgentReviewEnvelope {
        AgentReviewEnvelope {
            schema_version: 1,
            mode: report.mode.clone(),
            pr: report.pr,
            base: report.base.clone(),
            head: report.head.clone(),
            merge_base: report.merge_base.clone(),
            policy_digest: policy_digest.clone(),
            representation: rep,
            review_required_units: review_required_units.clone(),
            markers: markers.clone(),
            units,
            raw_diff,
            exempt_count: report.exempt.len(),
            exempt_units: exempt_units.clone(),
            skipped_count: report.skipped.len(),
            skipped_units: skipped_units.clone(),
            false_proven: report.false_proven.clone(),
            warnings: report.warnings.clone(),
            measurement: EnvelopeMeasurement {
                emitted_bytes: 0,
                audit_bytes,
                ratio: 0.0,
                token_cost: None,
                currency_cost: None,
                usage_reason: Some(
                    "token and currency costs unmeasured without a named provider adapter"
                        .to_owned(),
                ),
            },
        }
    };

    let mut cand_residual = create_candidate(
        Representation::Residual,
        Some(report.residual.clone()),
        None,
    );
    let bytes_res = serde_json::to_string_pretty(&cand_residual).map_or(usize::MAX, |s| s.len());
    cand_residual.measurement.emitted_bytes = bytes_res;
    cand_residual.measurement.ratio = calc_ratio(bytes_res, audit_bytes);

    let mut cand_raw = create_candidate(Representation::RawDiff, None, Some(raw_diff_str));
    let bytes_raw = serde_json::to_string_pretty(&cand_raw).map_or(usize::MAX, |s| s.len());
    cand_raw.measurement.emitted_bytes = bytes_raw;
    cand_raw.measurement.ratio = calc_ratio(bytes_raw, audit_bytes);

    if bytes_raw < bytes_res {
        cand_raw
    } else {
        cand_residual
    }
}
