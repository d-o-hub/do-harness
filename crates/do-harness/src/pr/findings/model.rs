//! Data types for PR findings provenance and issue tracking.

use serde::{Deserialize, Serialize};

/// Severity of a finding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum FindingSeverity {
    Info,
    Warning,
    Error,
    Critical,
}

impl FindingSeverity {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Info => "INFO",
            Self::Warning => "WARNING",
            Self::Error => "ERROR",
            Self::Critical => "CRITICAL",
        }
    }

    #[must_use]
    pub fn from_github(s: &str) -> Self {
        match s.to_ascii_lowercase().as_str() {
            "notice" | "info" => Self::Info,
            "failure" | "error" => Self::Error,
            "critical" | "high" => Self::Critical,
            // `warning` and any unrecognized level stay at the advisory tier:
            // an unknown level must never be silently downgraded to INFO.
            _ => Self::Warning,
        }
    }
}

/// Provenance classification of a finding against the verified baseline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum FindingClassification {
    Introduced,
    Regressed,
    PreExisting,
    Resolved,
    Unknown,
}

impl FindingClassification {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Introduced => "INTRODUCED",
            Self::Regressed => "REGRESSED",
            Self::PreExisting => "PRE_EXISTING",
            Self::Resolved => "RESOLVED",
            Self::Unknown => "UNKNOWN",
        }
    }
}

/// Mode for issue creation (`plan` or `create`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, clap::ValueEnum, Default)]
#[serde(rename_all = "lowercase")]
pub enum IssuesMode {
    #[default]
    Plan,
    Create,
}

/// One observed occurrence of a finding in CI (e.g. across matrix legs).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FindingOccurrence {
    pub check_run_id: Option<u64>,
    pub check_name: Option<String>,
    pub url: Option<String>,
}

/// Normalized deduplicated finding.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Finding {
    pub fingerprint: String,
    pub source_id: String,
    pub tool: String,
    #[serde(default)]
    pub tool_version: Option<String>,
    pub rule_id: String,
    pub severity: FindingSeverity,
    pub path: String,
    #[serde(default)]
    pub start_line: Option<u64>,
    #[serde(default)]
    pub end_line: Option<u64>,
    pub message: String,
    #[serde(default)]
    pub occurrences: Vec<FindingOccurrence>,
    pub classification: FindingClassification,
    #[serde(default)]
    pub issue_url: Option<String>,
    #[serde(default)]
    pub disposition: Option<String>,
}

/// Findings report emitted by `do-harness pr findings`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FindingsReport {
    pub schema_version: u32,
    pub pr: u64,
    pub head_sha: String,
    pub base_sha: String,
    pub base_verified: bool,
    pub policy: String,
    pub completeness: String,
    pub warnings: usize,
    pub baseline_findings: usize,
    pub untracked_findings: usize,
    pub issues_created: Vec<u64>,
    pub findings: Vec<Finding>,
    pub findings_digest: String,
}
