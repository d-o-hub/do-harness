//! The versioned client-capability report and its fixed reason codes.
//!
//! Nothing an evidence document or an adapter produced is rendered here: only
//! closed status/reason codes and fixed limitation text, so a report cannot
//! leak tokens, paths, prompts, or tool arguments.

use std::collections::BTreeMap;

use super::profile::{CLAIM_IDS, ClientPin};

/// Fixed reason code for a claim result; never free-form text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Reason {
    Observed,
    NotObserved,
    GuidanceOnly,
    UnsupportedClient,
    ConformanceUnknown,
    StubBuild,
    DegradedProxy,
    AdapterFailed,
    AdapterTimeout,
    MalformedObservation,
    OversizedObservation,
    StaleObservation,
    BindingMismatch,
    NoObservation,
    RouteMismatch,
}

impl Reason {
    pub(super) fn parse(code: &str) -> Option<Self> {
        Some(match code {
            "observed" => Self::Observed,
            "not-observed" => Self::NotObserved,
            "guidance-only" => Self::GuidanceOnly,
            "unsupported-client" => Self::UnsupportedClient,
            "conformance-unknown" => Self::ConformanceUnknown,
            _ => return None,
        })
    }

    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::Observed => "observed",
            Self::NotObserved => "not-observed",
            Self::GuidanceOnly => "guidance-only",
            Self::UnsupportedClient => "unsupported-client",
            Self::ConformanceUnknown => "conformance-unknown",
            Self::StubBuild => "stub-build",
            Self::DegradedProxy => "degraded-proxy",
            Self::AdapterFailed => "adapter-failed",
            Self::AdapterTimeout => "adapter-timeout",
            Self::MalformedObservation => "malformed-observation",
            Self::OversizedObservation => "oversized-observation",
            Self::StaleObservation => "stale-observation",
            Self::BindingMismatch => "binding-mismatch",
            Self::NoObservation => "no-observation",
            Self::RouteMismatch => "route-mismatch",
        }
    }
}

/// Outcome of one claim. Only `pass` satisfies a required claim.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ClaimStatus {
    Pass,
    Fail,
    Unknown,
    Stub,
    Degraded,
    Unsupported,
    Stale,
    Invalidated,
}

impl ClaimStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::Pass => "pass",
            Self::Fail => "fail",
            Self::Unknown => "unknown",
            Self::Stub => "stub",
            Self::Degraded => "degraded",
            Self::Unsupported => "unsupported",
            Self::Stale => "stale",
            Self::Invalidated => "invalidated",
        }
    }

    pub(super) fn is_pass(self) -> bool {
        matches!(self, Self::Pass)
    }
}

/// One claim in the report.
#[derive(Debug, Clone)]
pub(crate) struct ClaimResult {
    pub(super) required: bool,
    pub(super) route: Option<String>,
    pub(super) status: ClaimStatus,
    pub(super) reason: Reason,
    pub(super) source: &'static str,
    pub(super) limitation: &'static str,
}

/// The versioned client-capability report `doctor` emits.
#[derive(Debug)]
pub(crate) struct ClientReport {
    pub(super) client: ClientPin,
    pub(super) configuration: String,
    pub(super) adapter_sha256: Option<String>,
    pub(super) source: &'static str,
    pub(super) generated_at_unix_seconds: Option<u64>,
    pub(super) age_seconds: Option<u64>,
    pub(super) claims: BTreeMap<String, ClaimResult>,
}

impl ClientReport {
    /// `fail` when a required claim is not a pass, `unknown` when only advisory
    /// claims are unresolved, `pass` otherwise.
    pub(crate) fn verdict(&self) -> &'static str {
        if self
            .claims
            .values()
            .any(|claim| claim.required && !claim.status.is_pass())
        {
            "fail"
        } else if self.claims.values().any(|claim| !claim.status.is_pass()) {
            "unknown"
        } else {
            "pass"
        }
    }

    /// Messages for required claims that did not pass, for `--strict`.
    pub(crate) fn required_failures(&self) -> Vec<String> {
        self.claims
            .iter()
            .filter(|(_, claim)| claim.required && !claim.status.is_pass())
            .map(|(id, claim)| {
                format!(
                    "client claim '{id}' is {} ({})",
                    claim.status.as_str(),
                    claim.reason.as_str()
                )
            })
            .collect()
    }

    /// The versioned JSON report.
    pub(crate) fn to_json(&self) -> serde_json::Value {
        let claims = self
            .claims
            .iter()
            .map(|(id, claim)| {
                (
                    id.clone(),
                    serde_json::json!({
                        "required": claim.required,
                        "route": claim.route,
                        "status": claim.status.as_str(),
                        "reason": claim.reason.as_str(),
                        "source": claim.source,
                        "limitation": claim.limitation,
                    }),
                )
            })
            .collect::<serde_json::Map<String, serde_json::Value>>();
        serde_json::json!({
            "schema_version": 1,
            "claim": "client-capability",
            "client": { "id": self.client.id, "version": self.client.version },
            "adapter": { "sha256": self.adapter_sha256 },
            "configuration": self.configuration,
            "observation": {
                "source": self.source,
                "generated_at_unix_seconds": self.generated_at_unix_seconds,
                "age_seconds": self.age_seconds,
            },
            "claims": serde_json::Value::Object(claims),
            "verdict": self.verdict(),
        })
    }

    /// Human-readable lines, in the protocol's claim order.
    pub(crate) fn render_text(&self) -> Vec<String> {
        let mut lines = vec![format!(
            "  Client capability profile ({} evidence, client {} {}, adapter {}):",
            self.source,
            self.client.id,
            self.client.version,
            self.adapter_sha256.as_deref().unwrap_or("unpinned"),
        )];
        for id in CLAIM_IDS {
            let Some(claim) = self.claims.get(id) else {
                continue;
            };
            let mark = if claim.status.is_pass() {
                "OK"
            } else if claim.required {
                "FAIL"
            } else {
                "WARN"
            };
            let scope = if claim.required {
                "required"
            } else {
                "advisory"
            };
            let route = claim
                .route
                .as_deref()
                .map_or(String::new(), |route| format!(", route={route}"));
            lines.push(format!(
                "  [{mark}] {id} ({scope}{route}): {} ({}) - {}",
                claim.status.as_str(),
                claim.reason.as_str(),
                claim.limitation
            ));
        }
        lines
    }
}

/// Fixed limitation text per claim: nothing from an evidence document is
/// rendered, so a report cannot leak adapter or client output.
pub(super) fn limitation(id: &str) -> &'static str {
    match id {
        "guidance_installed" => {
            "Installed guidance says nothing about runtime mediation or permissions."
        }
        "host_boundary" => {
            "Only the tested route under the pinned client, adapter and configuration."
        }
        "mcp_mediation" => "Only the named MCP route; local shell and file tools are not covered.",
        "governance_enforced" => {
            "Only the tested proxy route; other traffic and clients bypass it."
        }
        "completion_gated" => "Only the tested completion path under the recorded adapter.",
        "context_sanitization" => {
            "One sanitizer fixture from the supplied client-conformance report."
        }
        "outbound_mediation" => {
            "Only named instrumented routes in the supplied conformance report."
        }
        _ => "Unrecognised claim.",
    }
}
