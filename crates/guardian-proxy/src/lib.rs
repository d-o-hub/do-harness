//! Optional guardian proxy library — adjacent to the dev harness.
//!
//! Always compiled; when `agt-governance` is off, decisions are permissive
//! (stub). When on, they delegate to `agent-governance` via an internal
//! mediator and stay fail-closed.

#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};

/// Proxy configuration (deny-unknown-fields for forward compatibility).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ProxyConfig {
    /// Address to bind, e.g. `127.0.0.1:8787`.
    pub bind: String,
    /// Upstream to forward allowed calls to, e.g. `http://127.0.0.1:9000`.
    pub upstream: String,
    /// Logical agent id for governance checks.
    pub agent_id: String,
    /// Optional path for hash-chained audit log (JSONL).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audit_log: Option<String>,
    /// Explicit host allowlist for the upstream. Empty means "any host except
    /// link-local/metadata and, unless opted in, private/loopback".
    #[serde(default)]
    pub upstream_allowlist: Vec<String>,
    /// Permit loopback/private upstreams (development sidecars).
    #[serde(default)]
    pub allow_private_upstreams: bool,
    /// Optional bearer token required on `GET /metrics`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metrics_token: Option<String>,
    /// Bearer token required on the mediation ingress (`POST /mcp` and the
    /// deprecated `POST /mcp/tools/call`) when set. `None` leaves the ingress
    /// open, which is the loopback sidecar default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ingress_token: Option<String>,
    /// Browser origins allowed on the MCP endpoint, matched per RFC 6454
    /// `(scheme, host, port)`. Requests without an `Origin` header always
    /// pass; an empty list disables Origin validation.
    #[serde(default = "default_allowed_origins")]
    pub allowed_origins: Vec<String>,
}

/// Loopback browser origins accepted on the MCP endpoint by default.
///
/// Browser clients on other ports must add their exact origin; non-browser
/// MCP clients do not send `Origin` and are unaffected.
pub(crate) fn default_allowed_origins() -> Vec<String> {
    [
        "http://localhost",
        "https://localhost",
        "http://127.0.0.1",
        "https://127.0.0.1",
    ]
    .iter()
    .map(|origin| (*origin).to_string())
    .collect()
}

/// Decision for a proxied tool call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ForwardDecision {
    /// Forward to upstream.
    Allow,
    /// Deny without forwarding (fail-closed).
    Deny {
        /// Human-readable reason.
        reason: String,
    },
}

/// Governance enforcement mode compiled into this proxy build.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GovernanceMode {
    /// `agt-governance` is compiled in; decisions go through the mediator.
    Enforced,
    /// `agt-governance` is not compiled in; well-formed calls are permitted.
    Stub,
}

impl GovernanceMode {
    /// Stable identifier used in diagnostics and reports.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Enforced => "enforced",
            Self::Stub => "stub",
        }
    }
}

/// Reports the governance mode this build actually implements.
///
/// A default build is a stub: it permits well-formed calls, so no report may
/// treat it as enforcement.
#[must_use]
pub const fn governance_mode() -> GovernanceMode {
    if cfg!(feature = "agt-governance") {
        GovernanceMode::Enforced
    } else {
        GovernanceMode::Stub
    }
}

mod proxy;
pub use proxy::{McpLikeToolCall, ProxyMediator, validate_upstream};

mod audit_log;
pub use audit_log::{AuditLog, AuditRecord};

mod error;
pub use error::{GuardianError, Result};

mod metrics;
pub use metrics::{MetricsSnapshot, ProxyMetrics};

mod state;
pub use state::AppState;

#[cfg(feature = "mcp-surface")]
pub mod mcp;

mod server;
pub use server::{
    create_router, create_router_degraded, create_router_with_audit, create_router_with_state,
};

#[cfg(test)]
mod tests {
    use super::{GovernanceMode, governance_mode};

    #[cfg(not(feature = "agt-governance"))]
    #[test]
    fn default_build_reports_stub_governance() {
        assert_eq!(governance_mode(), GovernanceMode::Stub);
        assert_eq!(governance_mode().as_str(), "stub");
    }

    #[cfg(feature = "agt-governance")]
    #[test]
    fn governance_build_reports_enforced() {
        assert_eq!(governance_mode(), GovernanceMode::Enforced);
        assert_eq!(governance_mode().as_str(), "enforced");
    }
}
