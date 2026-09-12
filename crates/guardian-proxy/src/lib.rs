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
