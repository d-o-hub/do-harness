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

/// Errors for proxy operations.
#[derive(Debug, thiserror::Error)]
pub enum ProxyError {
    /// Governance unavailable.
    #[error("governance check failed: {0}")]
    Governance(String),
}

mod proxy;
pub use proxy::{McpLikeToolCall, ProxyMediator};
