//! HTTP transport for the guardian proxy — `axum` router with fail-closed forwarding.

#![forbid(unsafe_code)]

use std::sync::Arc;

use axum::{
    Json, Router,
    body::Body,
    extract::{DefaultBodyLimit, State},
    http::{HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use serde_json::Value;

use crate::{AuditLog, McpLikeToolCall, ProxyMediator, state::AppState};

/// Maximum accepted request body (JSON tool call): one mebibyte.
const MAX_REQUEST_BYTES: usize = 1024 * 1024;
/// Maximum buffered upstream response body: one mebibyte.
const MAX_UPSTREAM_BYTES: usize = 1024 * 1024;
/// Response header advertising whether governance is enforced or stubbed.
const GOVERNANCE_HEADER: &str = "x-do-harness-governance";

/// Creates the `axum` router for the proxy.
///
/// Routes:
/// - `GET /health` — 200 only while the mediator is initialized; 503 degraded.
/// - `GET /metrics` — observability counters (bearer token when configured).
/// - `POST /mcp/tools/call` — tool-call mediation (fail-closed, forwards on Allow).
///
/// Request bodies are capped at one mebibyte; upstream responses are buffered up to
/// the same bound. There is intentionally no `POST /` alias: one mediation
/// entry point keeps the attack surface minimal.
pub fn create_router(mediator: Arc<ProxyMediator>) -> Router {
    create_router_with_state(AppState::new(mediator))
}

/// Creates router with audit logging.
pub fn create_router_with_audit(mediator: Arc<ProxyMediator>, audit: AuditLog) -> Router {
    create_router_with_state(AppState::with_audit(mediator, audit))
}

/// Creates a degraded router: `/health` is 503 and every tool call is denied.
pub fn create_router_degraded(error: String) -> Router {
    create_router_with_state(AppState::degraded(error))
}

/// Creates a router from explicit state (shares the state's metrics handle).
pub fn create_router_with_state(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health_handler))
        .route("/metrics", get(metrics_handler))
        .route("/mcp/tools/call", post(tool_call_handler))
        .layer(DefaultBodyLimit::max(MAX_REQUEST_BYTES))
        .with_state(state)
}

async fn health_handler(State(state): State<AppState>) -> Response {
    match &state.init_error {
        Some(error) => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({"status":"degraded","error":error})),
        )
            .into_response(),
        None => Json(serde_json::json!({"status":"ok"})).into_response(),
    }
}

async fn metrics_handler(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if let Some(token) = state.metrics_token.as_deref() {
        let authorized = headers
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.strip_prefix("Bearer "))
            .is_some_and(|presented| presented == token);
        if !authorized {
            return (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({"error":"missing or invalid metrics token"})),
            )
                .into_response();
        }
    }
    Json(state.metrics.snapshot()).into_response()
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
        guard.append(&call, &decision)
    })
    .await;
    match written {
        Ok(Ok(_record)) => {}
        Ok(Err(err)) => {
            state.metrics.inc_audit_write_failure();
            eprintln!("guardian-proxy: ALERT: audit append failed: {err}");
        }
        Err(join_err) => {
            state.metrics.inc_audit_write_failure();
            eprintln!("guardian-proxy: ALERT: audit task failed: {join_err}");
        }
    }
}

async fn tool_call_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(call): Json<McpLikeToolCall>,
) -> Response {
    if let Some(error) = &state.init_error {
        let body = serde_json::json!({"error": format!("governance unavailable: {error}")});
        return governance_header((StatusCode::SERVICE_UNAVAILABLE, Json(body)).into_response());
    }
    let Some(mediator) = &state.mediator else {
        let body = serde_json::json!({"error": "mediator unavailable"});
        return governance_header((StatusCode::SERVICE_UNAVAILABLE, Json(body)).into_response());
    };

    // Decision mapping is explicit and fail-closed: only an explicit Allow is
    // forwarded; denials and errors are returned as 403.
    let decision = match mediator.decide(&call) {
        Ok(d) => d,
        Err(e) => {
            let denied = crate::ForwardDecision::Deny {
                reason: format!("mediator error: {e}"),
            };
            state.metrics.inc_mediator_error();
            state.metrics.inc_deny();
            record_audit(&state, &call, &denied).await;
            let body = serde_json::json!({"error": format!("mediator error: {e}")});
            return governance_header((StatusCode::FORBIDDEN, Json(body)).into_response());
        }
    };

    if !cfg!(feature = "agt-governance") {
        state.metrics.inc_stub_decision();
    }
    match &decision {
        crate::ForwardDecision::Allow => state.metrics.inc_allow(),
        crate::ForwardDecision::Deny { .. } => state.metrics.inc_deny(),
    }
    record_audit(&state, &call, &decision).await;

    let response = match decision {
        crate::ForwardDecision::Deny { reason } => {
            let body = serde_json::json!({"error": reason});
            (StatusCode::FORBIDDEN, Json(body)).into_response()
        }
        crate::ForwardDecision::Allow => forward_to_upstream(&state, &call, &headers).await,
    };
    governance_header(response)
}

/// Tags a response with whether governance is enforced or stubbed.
fn governance_header(mut response: Response) -> Response {
    let mode = if cfg!(feature = "agt-governance") {
        "enforced"
    } else {
        "stub"
    };
    response
        .headers_mut()
        .insert(GOVERNANCE_HEADER, HeaderValue::from_static(mode));
    response
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
    // Buffer the upstream body under an explicit cap instead of trusting it.
    if upstream_resp
        .content_length()
        .is_some_and(|len| len > MAX_UPSTREAM_BYTES as u64)
    {
        state.metrics.inc_upstream_failure();
        let body = serde_json::json!({"error": "upstream response exceeds size limit"});
        return (StatusCode::BAD_GATEWAY, Json(body)).into_response();
    }
    let bytes = match upstream_resp.bytes().await {
        Ok(bytes) => bytes,
        Err(e) => {
            state.metrics.inc_upstream_failure();
            let body = serde_json::json!({"error": format!("upstream read failed: {e}")});
            return (StatusCode::BAD_GATEWAY, Json(body)).into_response();
        }
    };
    if bytes.len() > MAX_UPSTREAM_BYTES {
        state.metrics.inc_upstream_failure();
        let body = serde_json::json!({"error": "upstream response exceeds size limit"});
        return (StatusCode::BAD_GATEWAY, Json(body)).into_response();
    }
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
