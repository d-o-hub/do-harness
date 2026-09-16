//! MCP Streamable HTTP ingress for the guardian proxy (protocol 2026-07-28).

#![forbid(unsafe_code)]

use std::borrow::Cow;

use axum::Router;
use rmcp::model::{
    CallToolRequestParams, CallToolResponse, CallToolResult, ContentBlock, CustomRequest,
    CustomResult, Implementation, ListToolsResult, PaginatedRequestParams, ProtocolVersion,
    ServerCapabilities, ServerInfo,
};
use rmcp::service::RequestContext;
use rmcp::transport::streamable_http_server::session::local::LocalSessionManager;
use rmcp::transport::streamable_http_server::{StreamableHttpServerConfig, StreamableHttpService};
use rmcp::{ErrorData as McpError, RoleServer, ServerHandler};
use serde_json::{Value, json};

use crate::server::{MAX_REQUEST_BYTES, record_audit};
use crate::state::AppState;
use crate::{ForwardDecision, McpLikeToolCall};

mod forward;

/// Protocol revision served on the MCP endpoint.
pub(crate) const MCP_PROTOCOL_VERSION: &str = "2026-07-28";

/// Protocol revisions served on the MCP endpoint.
static SUPPORTED_VERSIONS: [ProtocolVersion; 1] = [ProtocolVersion::V_2026_07_28];

/// MCP server handler backed by the shared proxy state.
///
/// `tools/list` and custom methods are forwarded to the upstream endpoint.
/// `tools/call` is mediated: only an explicit `Allow` is forwarded, denials
/// and infrastructure errors fail closed.
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

    /// Fails closed while governance is unavailable.
    fn ensure_ready(&self) -> Result<(), McpError> {
        if let Some(error) = &self.state.init_error {
            return Err(McpError::internal_error(
                format!("governance unavailable: {error}"),
                None,
            ));
        }
        if self.state.mediator.is_none() {
            return Err(McpError::internal_error("mediator unavailable", None));
        }
        Ok(())
    }

    /// Runs the mediation pipeline for a tool call.
    ///
    /// Returns `Ok(Some(denial))` when the call is denied (audit and metrics
    /// already recorded), `Ok(None)` on allow, and `Err` for infrastructure
    /// failures; only `Ok(None)` may be forwarded.
    async fn mediate(&self, call: &McpLikeToolCall) -> Result<Option<CallToolResponse>, McpError> {
        let Some(mediator) = &self.state.mediator else {
            return Err(McpError::internal_error("mediator unavailable", None));
        };
        let decision = match mediator.decide(call) {
            Ok(decision) => decision,
            Err(err) => {
                let denied = ForwardDecision::Deny {
                    reason: format!("mediator error: {err}"),
                };
                self.state.metrics.inc_mediator_error();
                self.state.metrics.inc_deny();
                record_audit(&self.state, call, &denied).await;
                return Err(McpError::internal_error(
                    format!("mediator error: {err}"),
                    None,
                ));
            }
        };
        if !cfg!(feature = "agt-governance") {
            self.state.metrics.inc_stub_decision();
        }
        match &decision {
            ForwardDecision::Allow => self.state.metrics.inc_allow(),
            ForwardDecision::Deny { .. } => self.state.metrics.inc_deny(),
        }
        record_audit(&self.state, call, &decision).await;
        match decision {
            ForwardDecision::Deny { reason } => {
                let text = ContentBlock::text(format!("denied by governance: {reason}"));
                Ok(Some(CallToolResult::error(vec![text]).into()))
            }
            ForwardDecision::Allow => Ok(None),
        }
    }
}

impl ServerHandler for McpIngress {
    fn get_info(&self) -> ServerInfo {
        let mut info = ServerInfo::default();
        info.server_info = Implementation::new("guardian-proxy", env!("CARGO_PKG_VERSION"));
        info.capabilities = ServerCapabilities::builder().enable_tools().build();
        info
    }

    fn supported_protocol_versions(&self) -> Cow<'static, [ProtocolVersion]> {
        Cow::Borrowed(&SUPPORTED_VERSIONS)
    }

    async fn list_tools(
        &self,
        request: Option<PaginatedRequestParams>,
        context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, McpError> {
        self.ensure_ready()?;
        let params = match request {
            Some(request) => serde_json::to_value(&request).map_err(|err| {
                McpError::internal_error(format!("serialize tools/list: {err}"), None)
            })?,
            None => json!({}),
        };
        let result = forward::request(&self.state, "tools/list", None, params, &context.id).await?;
        serde_json::from_value(result).map_err(|err| {
            McpError::internal_error(format!("invalid tools/list result: {err}"), None)
        })
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, McpError> {
        self.ensure_ready()?;
        let call = McpLikeToolCall::new(
            request.name.to_string(),
            request.arguments.clone().map(Value::Object),
        );
        if let Some(denial) = self.mediate(&call).await? {
            return Ok(denial);
        }
        let params = serde_json::to_value(&request).map_err(|err| {
            McpError::internal_error(format!("serialize tools/call: {err}"), None)
        })?;
        let result = forward::request(
            &self.state,
            "tools/call",
            Some(request.name.as_ref()),
            params,
            &context.id,
        )
        .await?;
        let result: CallToolResult = serde_json::from_value(result).map_err(|err| {
            McpError::internal_error(format!("invalid tools/call result: {err}"), None)
        })?;
        Ok(result.into())
    }

    async fn on_custom_request(
        &self,
        request: CustomRequest,
        context: RequestContext<RoleServer>,
    ) -> Result<CustomResult, McpError> {
        self.ensure_ready()?;
        let params = request.params.unwrap_or_else(|| json!({}));
        let result =
            forward::request(&self.state, &request.method, None, params, &context.id).await?;
        Ok(CustomResult(result))
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
        .with_allowed_origins(state.allowed_origins.clone())
        .with_max_request_body_bytes(MAX_REQUEST_BYTES);
    let manager = LocalSessionManager::default().into();
    StreamableHttpService::new(move || Ok(McpIngress::new(state.clone())), manager, config)
}

/// Mounts the MCP endpoint at `/mcp` on `router`.
///
/// Generic over the router's state so the endpoint can be mounted before the
/// ingress-auth layer is applied (the nested transport itself needs no state).
pub fn mount<S>(router: Router<S>, state: AppState) -> Router<S>
where
    S: Clone + Send + Sync + 'static,
{
    router.nest_service("/mcp", service(state))
}

#[cfg(test)]
mod tests;
