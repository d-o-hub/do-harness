use super::{EvidenceArtifactRef, EvidenceSide};
use crate::evidence::EvidenceDocument;

pub(super) fn evidence_side(
    document: &EvidenceDocument,
    artifact: Option<&EvidenceArtifactRef>,
) -> EvidenceSide {
    let mut failed = Vec::new();
    let mut warned = Vec::new();
    let mut skipped = Vec::new();
    for sensor in &document.sensors {
        match sensor.verdict.as_str() {
            "fail" => failed.push(sensor.name.clone()),
            "warn" => warned.push(sensor.name.clone()),
            "skip" => skipped.push(sensor.name.clone()),
            _ => {}
        }
    }
    EvidenceSide {
        signal_set: document.signal_set.clone(),
        verdict: document.summary.verdict.clone(),
        workspace_fingerprint: document.workspace_fingerprint.clone(),
        policy_fingerprint: document.policy_fingerprint.clone(),
        failed,
        warned,
        skipped,
        artifact: artifact.cloned(),
    }
}
