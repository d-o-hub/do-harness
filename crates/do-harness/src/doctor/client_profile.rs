//! Opt-in client capability profile for `doctor`.
//!
//! A profile declares expected capabilities; evidence comes from a recorded
//! observation document, a client-conformance report, or an explicitly invoked
//! adapter. Declarations never satisfy a claim on their own, and the claims are
//! never collapsed into a single "safe" boolean.

mod observation;
mod probe;
mod profile;
mod report;

#[cfg(test)]
mod tests;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::exit::UsageError;

use observation::{ConformanceReport, Freshness, LoadOutcome, Observations, Unusable};
use profile::{ClaimRequirement, Profile};
pub(crate) use report::ClientReport;
use report::{ClaimResult, ClaimStatus, Reason, limitation};

/// One claim's evidence before it is attached to the profile's requirement.
#[derive(Debug, Clone, Copy)]
struct Evidence {
    status: ClaimStatus,
    reason: Reason,
    source: &'static str,
}

/// Evaluates a profile when one was supplied, leaving `doctor` untouched
/// otherwise.
///
/// # Errors
///
/// Propagates profile and probe setup errors; unusable evidence is reported as
/// a non-passing claim instead.
pub(crate) async fn evaluate_optional(
    profile_path: Option<&Path>,
    client_probe: bool,
) -> Result<Option<ClientReport>> {
    match profile_path {
        Some(path) => Ok(Some(evaluate(path, client_probe).await?)),
        None => Ok(None),
    }
}

/// Evaluates a profile against the current clock.
///
/// # Errors
///
/// See [`evaluate_optional`].
pub(crate) async fn evaluate(profile_path: &Path, client_probe: bool) -> Result<ClientReport> {
    evaluate_at(profile_path, client_probe, now_unix()).await
}

/// Evaluates a profile at an explicit time, so freshness is testable.
///
/// # Errors
///
/// See [`evaluate_optional`].
#[allow(clippy::too_many_lines)]
pub(crate) async fn evaluate_at(
    profile_path: &Path,
    client_probe: bool,
    now: u64,
) -> Result<ClientReport> {
    let profile = Profile::load(profile_path)?;
    let base = profile_path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));

    let mut source = "none";
    let mut generated_at = None;
    let mut evidence: BTreeMap<String, Evidence> = BTreeMap::new();
    let mut fallback: Option<Evidence> = None;

    if client_probe {
        let Some(adapter) = &profile.adapter else {
            return Err(UsageError::msg(
                "--client-probe requires an adapter in the client profile",
            )
            .into());
        };
        source = "probe";
        match probe::run(
            adapter,
            &profile.client,
            &profile.configuration,
            &profile.claims,
            now,
        )
        .await?
        {
            probe::ProbeOutcome::Observed(observations) => {
                generated_at = Some(observations.generated_at_unix_seconds);
                evidence.extend(recorded_evidence(&profile, &observations, now, source));
            }
            probe::ProbeOutcome::Failed(reason) => {
                fallback = Some(failure_evidence(reason, source));
            }
        }
    } else if let Some(relative) = &profile.observations {
        source = "recorded";
        match observation::load(&resolve(base, relative)) {
            LoadOutcome::Valid(observations) => {
                generated_at = Some(observations.generated_at_unix_seconds);
                evidence.extend(recorded_evidence(&profile, &observations, now, source));
            }
            LoadOutcome::Unusable(Unusable::Oversized) => {
                fallback = Some(failure_evidence(Reason::OversizedObservation, source));
            }
            LoadOutcome::Unusable(Unusable::Malformed) => {
                fallback = Some(failure_evidence(Reason::MalformedObservation, source));
            }
        }
    }

    if let Some(relative) = &profile.conformance_report {
        match observation::load_conformance(&resolve(base, relative)) {
            LoadOutcome::Valid(report) => {
                for (id, found) in conformance_evidence(&profile, &report) {
                    evidence.entry(id).or_insert(found);
                }
            }
            LoadOutcome::Unusable(_) => {
                for id in ["context_sanitization", "outbound_mediation"] {
                    if profile.claims.contains_key(id) {
                        evidence.entry(id.to_owned()).or_insert(failure_evidence(
                            Reason::MalformedObservation,
                            "conformance",
                        ));
                    }
                }
            }
        }
    }

    if let Some(fallback) = fallback {
        for id in profile.claims.keys() {
            evidence.entry(id.clone()).or_insert(fallback);
        }
    }

    let claims = profile
        .claims
        .iter()
        .map(|(id, requirement)| {
            let found = evidence.get(id).copied().unwrap_or(Evidence {
                status: ClaimStatus::Unknown,
                reason: Reason::NoObservation,
                source,
            });
            (
                id.clone(),
                ClaimResult {
                    required: requirement.required,
                    route: requirement.route.clone(),
                    status: found.status,
                    reason: found.reason,
                    source: found.source,
                    limitation: limitation(id),
                },
            )
        })
        .collect();

    Ok(ClientReport {
        client: profile.client.clone(),
        configuration: profile.configuration.clone(),
        adapter_sha256: profile
            .adapter
            .as_ref()
            .map(|adapter| adapter.sha256.clone()),
        source,
        generated_at_unix_seconds: generated_at,
        age_seconds: generated_at.map(|generated| now.saturating_sub(generated)),
        claims,
    })
}

/// The route a claim is expected to have been tested on.
pub(crate) fn expected_route<'a>(id: &str, requirement: &'a ClaimRequirement) -> Option<&'a str> {
    requirement.route.as_deref().or(match id {
        "guidance_installed" => Some("guidance"),
        _ => None,
    })
}

fn route_matches(id: &str, requirement: &ClaimRequirement, observed: &str) -> bool {
    expected_route(id, requirement).is_none_or(|expected| expected == observed)
}

fn recorded_evidence(
    profile: &Profile,
    observations: &Observations,
    now: u64,
    source: &'static str,
) -> BTreeMap<String, Evidence> {
    let adapter_pin = profile
        .adapter
        .as_ref()
        .map(|adapter| adapter.sha256.as_str());
    let bound = observations.client == profile.client
        && observations.configuration == profile.configuration
        && adapter_pin == Some(observations.adapter.sha256.as_str());
    let document_level = if bound {
        match observations.freshness(now) {
            Freshness::Fresh => None,
            Freshness::Stale => Some(Evidence {
                status: ClaimStatus::Stale,
                reason: Reason::StaleObservation,
                source,
            }),
            Freshness::Future => Some(failure_evidence(Reason::MalformedObservation, source)),
        }
    } else {
        Some(failure_evidence(Reason::BindingMismatch, source))
    };

    let mut evidence = BTreeMap::new();
    for (id, requirement) in &profile.claims {
        if let Some(document_level) = document_level {
            evidence.insert(id.clone(), document_level);
            continue;
        }
        // A claim the document does not record stays absent, so another
        // evidence source (the conformance report) can still supply it.
        if let Some(claim) = observations.claims.get(id) {
            evidence.insert(
                id.clone(),
                evaluate_claim(
                    id,
                    requirement,
                    claim,
                    observations.governance.as_ref(),
                    source,
                ),
            );
        }
    }
    evidence
}

fn evaluate_claim(
    id: &str,
    requirement: &ClaimRequirement,
    claim: &observation::ClaimObservation,
    governance: Option<&observation::GovernanceObservation>,
    source: &'static str,
) -> Evidence {
    if !route_matches(id, requirement, &claim.route) {
        return Evidence {
            status: ClaimStatus::Invalidated,
            reason: Reason::RouteMismatch,
            source,
        };
    }
    let status = match claim.status.as_str() {
        "pass" => ClaimStatus::Pass,
        "fail" => ClaimStatus::Fail,
        _ => ClaimStatus::Unknown,
    };
    let reason = Reason::parse(&claim.reason).unwrap_or(Reason::NotObserved);
    if status == ClaimStatus::Unknown && reason == Reason::UnsupportedClient {
        return Evidence {
            status: ClaimStatus::Unsupported,
            reason,
            source,
        };
    }
    if id == "governance_enforced" {
        return governance_evidence(claim, governance, status, reason, source);
    }
    if id == "guidance_installed" && status == ClaimStatus::Pass {
        return Evidence {
            status,
            reason: Reason::GuidanceOnly,
            source,
        };
    }
    Evidence {
        status,
        reason,
        source,
    }
}

fn governance_evidence(
    claim: &observation::ClaimObservation,
    governance: Option<&observation::GovernanceObservation>,
    status: ClaimStatus,
    reason: Reason,
    source: &'static str,
) -> Evidence {
    let Some(governance) = governance else {
        return Evidence {
            status: ClaimStatus::Unknown,
            reason: Reason::NotObserved,
            source,
        };
    };
    if governance.route != claim.route {
        return Evidence {
            status: ClaimStatus::Invalidated,
            reason: Reason::RouteMismatch,
            source,
        };
    }
    match governance.mode.as_str() {
        // A pass with no observation behind it is not evidence.
        "enforced" if status == ClaimStatus::Pass && reason != Reason::Observed => Evidence {
            status: ClaimStatus::Invalidated,
            reason: Reason::MalformedObservation,
            source,
        },
        "enforced" => Evidence {
            status,
            reason,
            source,
        },
        "stub" => Evidence {
            status: ClaimStatus::Stub,
            reason: Reason::StubBuild,
            source,
        },
        "degraded" => Evidence {
            status: ClaimStatus::Degraded,
            reason: Reason::DegradedProxy,
            source,
        },
        _ => Evidence {
            status: ClaimStatus::Unknown,
            reason: Reason::NotObserved,
            source,
        },
    }
}

fn conformance_evidence(
    profile: &Profile,
    report: &ConformanceReport,
) -> BTreeMap<String, Evidence> {
    let bound = report.client == profile.client;
    let mut evidence = BTreeMap::new();
    for (id, status) in [
        ("context_sanitization", &report.claims.sanitizer_fixture),
        ("outbound_mediation", &report.claims.client_boundary),
    ] {
        if !profile.claims.contains_key(id) {
            continue;
        }
        let found = if bound {
            match status.as_str() {
                "pass" => Evidence {
                    status: ClaimStatus::Pass,
                    reason: Reason::Observed,
                    source: "conformance",
                },
                "fail" => Evidence {
                    status: ClaimStatus::Fail,
                    reason: Reason::NotObserved,
                    source: "conformance",
                },
                _ => Evidence {
                    status: ClaimStatus::Unknown,
                    reason: Reason::ConformanceUnknown,
                    source: "conformance",
                },
            }
        } else {
            failure_evidence(Reason::BindingMismatch, "conformance")
        };
        evidence.insert(id.to_owned(), found);
    }
    evidence
}

fn failure_evidence(reason: Reason, source: &'static str) -> Evidence {
    let status = match reason {
        Reason::AdapterFailed | Reason::AdapterTimeout => ClaimStatus::Fail,
        _ => ClaimStatus::Invalidated,
    };
    Evidence {
        status,
        reason,
        source,
    }
}

fn resolve(base: &Path, relative: &str) -> PathBuf {
    let path = Path::new(relative);
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        base.join(path)
    }
}

fn now_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs())
}
