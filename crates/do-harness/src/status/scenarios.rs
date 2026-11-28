use super::{EvidenceState, StatusReport};
use crate::evidence::EvidenceDocument;
use crate::scenarios::ScenarioOutcome;

/// Revises a green decision when enforced scenarios lack recorded evidence.
///
/// An enforcing set is only green with recorded scenario outcomes: without
/// them the run that produced the artifact never exercised the scenarios, and
/// a recorded non-pass outcome is a failure the fingerprints cannot express.
pub(super) fn revise(
    decision: &mut StatusReport,
    enforced: bool,
    document: &EvidenceDocument,
    set: Option<&str>,
) {
    if decision.state != EvidenceState::Green || !enforced {
        return;
    }
    if document.signal_set.as_deref() != set || document.scenario_results.is_empty() {
        decision.state = EvidenceState::Missing;
        "scenario_evidence_missing".clone_into(&mut decision.reason);
        decision.recovery = Some(super::recovery_command(set));
    } else if document
        .scenario_results
        .iter()
        .any(|result| result.outcome != ScenarioOutcome::Passed)
    {
        decision.state = EvidenceState::Red;
        "scenario_checks_failed".clone_into(&mut decision.reason);
    }
}
