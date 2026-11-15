//! Text rendering for `status`: the human line and the error summary.

use std::fmt::Write as _;

use super::{EvidenceState, StatusReport};
use crate::evidence::running::{RunningInfo, describe};

/// Failure summary for the error stream, naming unusable sensors and the
/// command that clears the state.
pub(super) fn failure_summary(report: &StatusReport) -> String {
    if report.state == EvidenceState::Running {
        let info = report.running.clone().unwrap_or_default();
        return describe(report.set.as_deref(), &info);
    }
    let state = match report.state {
        EvidenceState::Red => "red",
        EvidenceState::Stale => "stale",
        _ => "missing",
    };
    let mut summary = format!("evidence is {state} ({})", report.reason);
    if !report.missing_sensors.is_empty() {
        let _ = write!(
            summary,
            "; missing sensors: {}",
            report.missing_sensors.join(", ")
        );
    }
    if let Some(evidence) = &report.evidence {
        let details = sensor_details(evidence);
        if !details.is_empty() {
            let _ = write!(summary, "; {}", details.join("; "));
        }
    }
    if let Some(recovery) = &report.recovery {
        let _ = write!(summary, "\n  → recover: {recovery}");
    }
    summary
}

/// Prints the human-readable one-line status plus actionable detail.
pub(super) fn print_text(report: &StatusReport) {
    let set = report.set.as_deref().unwrap_or("(default)");
    match report.state {
        EvidenceState::Green => {
            println!("green: signal set '{set}' evidence is current.");
        }
        EvidenceState::Red => {
            let details = report
                .evidence
                .as_ref()
                .map(sensor_details)
                .unwrap_or_default();
            if details.is_empty() {
                println!(
                    "red: signal set '{set}' evidence is not a current pass ({}).",
                    report.reason
                );
            } else {
                println!(
                    "red: signal set '{set}' evidence has {}.",
                    details.join("; ")
                );
            }
        }
        EvidenceState::Stale => {
            println!(
                "stale: signal set '{set}' evidence no longer applies ({}).",
                report.reason
            );
        }
        EvidenceState::Running => {
            let info: RunningInfo = report.running.clone().unwrap_or_default();
            println!("running: {}.", describe(report.set.as_deref(), &info));
        }
        EvidenceState::Missing => {
            println!(
                "missing: no current evidence for signal set '{set}' ({}).",
                report.reason
            );
        }
    }
}

fn sensor_details(evidence: &super::EvidenceSide) -> Vec<String> {
    let mut details = Vec::new();
    if !evidence.failed.is_empty() {
        details.push(format!("failures: {}", evidence.failed.join(", ")));
    }
    if !evidence.warned.is_empty() {
        details.push(format!("advisory sensors: {}", evidence.warned.join(", ")));
    }
    if !evidence.skipped.is_empty() {
        details.push(format!(
            "non-fresh sensors: {}",
            evidence.skipped.join(", ")
        ));
    }
    details
}
