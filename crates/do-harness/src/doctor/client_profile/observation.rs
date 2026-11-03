//! Recorded client observations, their profile binding, and the
//! client-conformance v1 subset `doctor` consumes.

use std::collections::BTreeMap;
use std::path::Path;

use serde::Deserialize;

use super::profile::{CLAIM_IDS, ClientPin, is_identifier, is_sha256_hex};

/// Only accepted observation schema version.
pub(crate) const OBSERVATION_SCHEMA_VERSION: u32 = 1;

/// Largest observation document accepted, in bytes.
const MAX_OBSERVATION_BYTES: usize = 256 * 1024;

/// Longest accepted freshness window (seven days).
const MAX_MAX_AGE_SECONDS: u64 = 7 * 24 * 60 * 60;

/// Tolerated clock skew for a `generated_at_unix_seconds` in the future.
pub(crate) const MAX_CLOCK_SKEW_SECONDS: u64 = 300;

/// Statuses an observation document may claim; anything else is derived here.
pub(crate) const OBSERVED_STATUSES: [&str; 3] = ["pass", "fail", "unknown"];

/// Fixed reason codes a recorded or probed observation may carry.
pub(crate) const OBSERVED_REASONS: [&str; 5] = [
    "observed",
    "not-observed",
    "guidance-only",
    "unsupported-client",
    "conformance-unknown",
];

/// Governance modes a proxy observation may report.
pub(crate) const GOVERNANCE_MODES: [&str; 4] = ["enforced", "stub", "degraded", "unknown"];

/// Pinned adapter identity an observation was produced under.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AdapterIdentity {
    pub(crate) sha256: String,
}

/// Governance mode observed on a tested route.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct GovernanceObservation {
    pub(crate) mode: String,
    pub(crate) route: String,
}

/// One observed claim: where it was tested and what was seen.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ClaimObservation {
    pub(crate) route: String,
    pub(crate) status: String,
    pub(crate) reason: String,
}

/// A recorded observation document.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Observations {
    pub(crate) schema_version: u32,
    pub(crate) client: ClientPin,
    pub(crate) adapter: AdapterIdentity,
    pub(crate) configuration: String,
    pub(crate) generated_at_unix_seconds: u64,
    pub(crate) max_age_seconds: u64,
    #[serde(default)]
    pub(crate) governance: Option<GovernanceObservation>,
    pub(crate) claims: BTreeMap<String, ClaimObservation>,
}

/// How fresh a recorded observation is relative to the current clock.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Freshness {
    Fresh,
    Stale,
    Future,
}

/// Outcome of reading an evidence document.
#[derive(Debug)]
pub(crate) enum LoadOutcome<T> {
    Valid(T),
    Unusable(Unusable),
}

/// Why a document could not be used as evidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Unusable {
    Malformed,
    Oversized,
}

impl Observations {
    fn validate(&self) -> std::result::Result<(), String> {
        if self.schema_version != OBSERVATION_SCHEMA_VERSION {
            return Err(format!(
                "unsupported schema_version {}",
                self.schema_version
            ));
        }
        if !is_identifier(&self.client.id) || !is_identifier(&self.client.version) {
            return Err("client identity must be a bounded identifier".to_owned());
        }
        if !is_sha256_hex(&self.adapter.sha256) {
            return Err("adapter.sha256 must be 64 lowercase hexadecimal characters".to_owned());
        }
        if !is_identifier(&self.configuration) {
            return Err("configuration must be a bounded identifier".to_owned());
        }
        if self.max_age_seconds == 0 || self.max_age_seconds > MAX_MAX_AGE_SECONDS {
            return Err(format!("max_age_seconds must be 1..={MAX_MAX_AGE_SECONDS}"));
        }
        if let Some(governance) = &self.governance {
            if !GOVERNANCE_MODES.contains(&governance.mode.as_str()) {
                return Err("governance.mode is not a known mode".to_owned());
            }
            if !is_identifier(&governance.route) {
                return Err("governance.route must be a bounded identifier".to_owned());
            }
        }
        if self.claims.is_empty() {
            return Err("records no claims".to_owned());
        }
        for (id, claim) in &self.claims {
            if !CLAIM_IDS.contains(&id.as_str()) {
                return Err(format!("records unknown claim '{id}'"));
            }
            if !OBSERVED_STATUSES.contains(&claim.status.as_str()) {
                return Err(format!(
                    "claim '{id}' reports status '{}', which only do-harness may derive",
                    claim.status
                ));
            }
            if !OBSERVED_REASONS.contains(&claim.reason.as_str()) {
                return Err(format!(
                    "claim '{id}' reports unknown reason '{}'",
                    claim.reason
                ));
            }
            if !is_identifier(&claim.route) {
                return Err(format!("claim '{id}' route must be a bounded identifier"));
            }
            // Installed guidance is its own claim; it cannot stand in for one.
            if claim.reason == "guidance-only" && id != "guidance_installed" {
                return Err(format!(
                    "claim '{id}' cannot be satisfied by installed guidance"
                ));
            }
        }
        Ok(())
    }

    pub(crate) fn freshness(&self, now: u64) -> Freshness {
        if self.generated_at_unix_seconds > now.saturating_add(MAX_CLOCK_SKEW_SECONDS) {
            return Freshness::Future;
        }
        if self.age_seconds(now) > self.max_age_seconds {
            Freshness::Stale
        } else {
            Freshness::Fresh
        }
    }

    pub(crate) fn age_seconds(&self, now: u64) -> u64 {
        now.saturating_sub(self.generated_at_unix_seconds)
    }
}

/// Reads a recorded observation document; unusable evidence is not an error.
pub(crate) fn load(path: &Path) -> LoadOutcome<Observations> {
    decode(path).map_or_else(LoadOutcome::Unusable, |parsed: Observations| {
        match parsed.validate() {
            Ok(()) => LoadOutcome::Valid(parsed),
            Err(_) => LoadOutcome::Unusable(Unusable::Malformed),
        }
    })
}

fn decode<T: serde::de::DeserializeOwned>(path: &Path) -> std::result::Result<T, Unusable> {
    let Ok(bytes) = std::fs::read(path) else {
        return Err(Unusable::Malformed);
    };
    if bytes.len() > MAX_OBSERVATION_BYTES {
        return Err(Unusable::Oversized);
    }
    let Ok(raw) = String::from_utf8(bytes) else {
        return Err(Unusable::Malformed);
    };
    serde_json::from_str(&raw).map_err(|_| Unusable::Malformed)
}

/// The client-conformance v1 report subset do-harness consumes.
///
/// The full schema is validated by `scripts/check-client-conformance.py`; here
/// the document only has to be intact enough to trust its privacy claims.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ConformanceReport {
    pub(crate) schema_version: u32,
    pub(crate) claim: String,
    pub(crate) client: ClientPin,
    pub(crate) integration_method: String,
    pub(crate) test_suite_version: String,
    pub(crate) timestamp: String,
    pub(crate) claims: ConformanceClaims,
    pub(crate) scenarios: Vec<serde_json::Value>,
    pub(crate) verdict: String,
    pub(crate) limitations: Vec<String>,
}

/// The privacy claims a conformance report actually measures.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ConformanceClaims {
    pub(crate) guidance_installed: String,
    pub(crate) sanitizer_fixture: String,
    pub(crate) client_boundary: String,
}

/// Number of synthetic scenarios protocol v1 always runs.
const CONFORMANCE_SCENARIOS: usize = 4;

impl ConformanceReport {
    fn validate(&self) -> std::result::Result<(), String> {
        if self.schema_version != 1 || self.claim != "client-boundary" {
            return Err("not a client-conformance v1 report".to_owned());
        }
        if !is_identifier(&self.client.id) || !is_identifier(&self.client.version) {
            return Err("client identity must be a bounded identifier".to_owned());
        }
        if self.integration_method.is_empty()
            || self.test_suite_version != "1"
            || self.timestamp.is_empty()
        {
            return Err("report metadata is incomplete".to_owned());
        }
        if self.scenarios.len() != CONFORMANCE_SCENARIOS {
            return Err(format!(
                "expected {CONFORMANCE_SCENARIOS} synthetic scenarios"
            ));
        }
        if !OBSERVED_STATUSES.contains(&self.verdict.as_str()) {
            return Err("verdict is not a known status".to_owned());
        }
        if self.limitations.is_empty() {
            return Err("report states no limitations".to_owned());
        }
        for status in [
            &self.claims.guidance_installed,
            &self.claims.sanitizer_fixture,
            &self.claims.client_boundary,
        ] {
            if !OBSERVED_STATUSES.contains(&status.as_str()) {
                return Err("claim status is not a known status".to_owned());
            }
        }
        Ok(())
    }
}

/// Reads a client-conformance report; unusable evidence is not an error.
pub(crate) fn load_conformance(path: &Path) -> LoadOutcome<ConformanceReport> {
    decode(path).map_or_else(
        LoadOutcome::Unusable,
        |parsed: ConformanceReport| match parsed.validate() {
            Ok(()) => LoadOutcome::Valid(parsed),
            Err(_) => LoadOutcome::Unusable(Unusable::Malformed),
        },
    )
}
