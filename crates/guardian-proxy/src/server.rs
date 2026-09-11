//! HTTP transport for the guardian proxy — `axum` router with fail-closed forwarding.

#![forbid(unsafe_code)]

use std::sync::Arc;

use axum::{
    Json, Router,
    body::Body,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use serde_json::Value;

use crate::{AuditLog, McpLikeToolCall, ProxyMediator, state::AppState};

/// Creates the `axum` router for the proxy.
///
/// Routes:
/// - `GET /health` — liveness probe (always 200)
/// - `GET /metrics` — observability counters (always 200)
/// - `POST /mcp/tools/call` — tool-call mediation (fail-closed, forwards on Allow)
/// - `POST /` — alias for `/mcp/tools/call` (compatibility)
pub fn create_router(mediator: Arc<ProxyMediator>) -> Router {
    create_router_with_state(AppState::new(mediator))
}

/// Creates router with audit logging.
pub fn create_router_with_audit(mediator: Arc<ProxyMediator>, audit: AuditLog) -> Router {
    create_router_with_state(AppState::with_audit(mediator, audit))
}

/// Creates a router from explicit state (shares the state's metrics handle).
pub fn create_router_with_state(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health_handler))
        .route("/metrics", get(metrics_handler))
        .route("/mcp/tools/call", post(tool_call_handler))
        .route("/", post(tool_call_handler))
        .with_state(state)
}

async fn health_handler() -> impl IntoResponse {
    Json(serde_json::json!({"status":"ok"}))
}

async fn metrics_handler(State(state): State<AppState>) -> impl IntoResponse {
    Json(state.metrics.snapshot())
}

async fn record_audit(state: &AppState, call: &McpLikeToolCall, decision: &crate::ForwardDecision) {
    let Some(audit) = state.audit.clone() else {
        return;
    };
    // The audit log uses synchronous std::fs; append it on a blocking thread
    // so the current-thread executor is never stalled by disk I/O.
    let call = call.clone();
    let decision = decision.clone();
    let written = tokio::task::spawn_blocking(move || {
        let mut guard = audit.blocking_lock();
        guard.append(&call, &decision).is_ok()
    })
    .await;
    if !matches!(written, Ok(true)) {
        state.metrics.inc_audit_write_failure();
    }
}

async fn tool_call_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(call): Json<McpLikeToolCall>,
) -> Response {
    let decision = match state.mediator.decide(&call) {
        Ok(d) => d,
        Err(e) => {
            let denied = crate::ForwardDecision::Deny {
                reason: format!("mediator error: {e}"),
            };
            state.metrics.inc_mediator_error();
            state.metrics.inc_deny();
            record_audit(&state, &call, &denied).await;
            let body = serde_json::json!({"error": format!("mediator error: {e}")});
            return (StatusCode::FORBIDDEN, Json(body)).into_response();
        }
    };

    match &decision {
        crate::ForwardDecision::Allow => state.metrics.inc_allow(),
        crate::ForwardDecision::Deny { .. } => state.metrics.inc_deny(),
    }
    record_audit(&state, &call, &decision).await;

    match decision {
        crate::ForwardDecision::Deny { reason } => {
            let body = serde_json::json!({"error": reason});
            (StatusCode::FORBIDDEN, Json(body)).into_response()
        }
        crate::ForwardDecision::Allow => forward_to_upstream(&state, &call, &headers).await,
    }
}

async fn forward_to_upstream(
    state: &AppState,
    call: &McpLikeToolCall,
    headers: &HeaderMap,
) -> Response {
    let url = state.upstream.clone();
    // Forward JSON body to upstream via POST.
    let mut req = state.client.post(&url).json(call);

    // Propagate content-type if caller set it; reqwest already sets json header, so keep minimal.
    if let Some(ct) = headers.get("content-type").and_then(|v| v.to_str().ok()) {
        req = req.header("content-type", ct);
    }

    let upstream_resp = match req.send().await {
        Ok(r) => r,
        Err(e) => {
            state.metrics.inc_upstream_failure();
            let body = serde_json::json!({"error": format!("upstream unreachable: {e}")});
            return (StatusCode::BAD_GATEWAY, Json(body)).into_response();
        }
    };

    let status =
        StatusCode::from_u16(upstream_resp.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
    let bytes = match upstream_resp.bytes().await {
        Ok(b) => b,
        Err(e) => {
            state.metrics.inc_upstream_failure();
            let body = serde_json::json!({"error": format!("upstream read failed: {e}")});
            return (StatusCode::BAD_GATEWAY, Json(body)).into_response();
        }
    };
    state.metrics.inc_upstream_ok();

    // Try to return JSON if upstream returned JSON, otherwise raw bytes.
    if let Ok(json_val) = serde_json::from_slice::<Value>(&bytes) {
        let mut resp = Json(json_val).into_response();
        *resp.status_mut() = status;
        resp
    } else {
        let mut resp = Response::new(Body::from(bytes.to_vec()));
        *resp.status_mut() = status;
        resp
    }
}

#[cfg(test)]
mod tests;
