//! MCP Streamable HTTP ingress for the guardian proxy (protocol 2026-07-28).

#![forbid(unsafe_code)]

use std::borrow::Cow;

use axum::Router;
use rmcp::ServerHandler;
use rmcp::model::{Implementation, ProtocolVersion, ServerInfo};
use rmcp::transport::streamable_http_server::session::local::LocalSessionManager;
use rmcp::transport::streamable_http_server::{StreamableHttpServerConfig, StreamableHttpService};

use crate::server::MAX_REQUEST_BYTES;
use crate::state::AppState;

/// Protocol revisions served on the MCP endpoint.
static SUPPORTED_VERSIONS: [ProtocolVersion; 1] = [ProtocolVersion::V_2026_07_28];

/// MCP server handler backed by the shared proxy state.
///
/// The ingress scaffold answers discovery and lifecycle requests. Tool
/// forwarding and fail-closed mediation on the MCP path are layered on in
/// `feat-mcp-tools-forwarding`; until then the endpoint executes nothing.
#[derive(Clone)]
pub struct McpIngress {
    state: AppState,
}

impl McpIngress {
    /// Creates a handler over the shared proxy state.
    #[must_use]
    pub fn new(state: AppState) -> Self {
        Self { state }
    }

    /// Shared proxy state (mediator, upstream client, audit, metrics).
    #[must_use]
    pub fn state(&self) -> &AppState {
        &self.state
    }
}

impl ServerHandler for McpIngress {
    fn get_info(&self) -> ServerInfo {
        let mut info = ServerInfo::default();
        info.server_info = Implementation::new("guardian-proxy", env!("CARGO_PKG_VERSION"));
        info
    }

    fn supported_protocol_versions(&self) -> Cow<'static, [ProtocolVersion]> {
        Cow::Borrowed(&SUPPORTED_VERSIONS)
    }
}

/// Builds the Streamable HTTP service: modern-only, stateless, JSON responses.
///
/// Per-request protocol metadata is required, sessions are disabled, and the
/// request body is capped at the same one-mebibyte bound as the legacy route.
#[must_use]
pub fn service(state: AppState) -> StreamableHttpService<McpIngress, LocalSessionManager> {
    let config = StreamableHttpServerConfig::default()
        .with_legacy_session_mode(false)
        .with_json_response(true)
        .with_stateless_protocol_metadata_required(true)
        .with_max_request_body_bytes(MAX_REQUEST_BYTES);
    let manager = LocalSessionManager::default().into();
    StreamableHttpService::new(move || Ok(McpIngress::new(state.clone())), manager, config)
}

/// Mounts the MCP endpoint at `/mcp` on `router`.
pub fn mount(router: Router, state: AppState) -> Router {
    router.nest_service("/mcp", service(state))
}

#[cfg(test)]
mod tests;
