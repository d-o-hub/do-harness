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
pub use proxy::{McpLikeToolCall, ProxyMediator};

mod audit_log;
pub use audit_log::{AuditLog, AuditRecord};

mod server;
pub use server::{AppState, create_router, create_router_with_audit};
