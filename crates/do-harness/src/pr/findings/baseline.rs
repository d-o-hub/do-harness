//! Baseline comparison and provenance classification.

use std::collections::BTreeMap;

use super::model::{Finding, FindingClassification};

/// Compares head findings against base findings and classifies each finding.
pub fn compare_baseline(
    head_findings: &mut [Finding],
    base_findings: &[Finding],
    base_verified: bool,
) {
    if !base_verified {
        for f in head_findings.iter_mut() {
            f.classification = FindingClassification::Unknown;
        }
        return;
    }

    let base_map: BTreeMap<&str, &Finding> = base_findings
        .iter()
        .map(|f| (f.fingerprint.as_str(), f))
        .collect();

    for f in head_findings.iter_mut() {
        if let Some(base_f) = base_map.get(f.fingerprint.as_str()) {
            if f.severity > base_f.severity {
                f.classification = FindingClassification::Regressed;
            } else {
                f.classification = FindingClassification::PreExisting;
            }
        } else {
            f.classification = FindingClassification::Introduced;
        }
    }
}
