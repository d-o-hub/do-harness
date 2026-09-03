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

use crate::{AuditLog, McpLikeToolCall, ProxyMediator};

/// Shared state for the router.
#[derive(Clone)]
pub struct AppState {
    /// Mediator that decides allow/deny.
    pub mediator: Arc<ProxyMediator>,
    /// Upstream base URL.
    pub upstream: String,
    /// HTTP client for forwarding allowed calls.
    pub client: reqwest::Client,
    /// Optional audit log for decision evidence.
    pub audit: Option<Arc<tokio::sync::Mutex<AuditLog>>>,
}

impl AppState {
    /// Creates state from a mediator.
    #[must_use]
    pub fn new(mediator: Arc<ProxyMediator>) -> Self {
        let upstream = mediator.upstream().to_string();
        let client = reqwest::Client::new();
        Self {
            mediator,
            upstream,
            client,
            audit: None,
        }
    }

    /// Creates state with audit log.
    #[must_use]
    pub fn with_audit(mediator: Arc<ProxyMediator>, audit: AuditLog) -> Self {
        let upstream = mediator.upstream().to_string();
        let client = reqwest::Client::new();
        Self {
            mediator,
            upstream,
            client,
            audit: Some(Arc::new(tokio::sync::Mutex::new(audit))),
        }
    }
}

/// Creates the `axum` router for the proxy.
///
/// Routes:
/// - `GET /health` — liveness probe (always 200)
/// - `POST /mcp/tools/call` — tool-call mediation (fail-closed, forwards on Allow)
/// - `POST /` — alias for `/mcp/tools/call` (compatibility)
pub fn create_router(mediator: Arc<ProxyMediator>) -> Router {
    let state = AppState::new(mediator);
    Router::new()
        .route("/health", get(health_handler))
        .route("/mcp/tools/call", post(tool_call_handler))
        .route("/", post(tool_call_handler))
        .with_state(state)
}

/// Creates router with audit logging.
pub fn create_router_with_audit(mediator: Arc<ProxyMediator>, audit: AuditLog) -> Router {
    let state = AppState::with_audit(mediator, audit);
    Router::new()
        .route("/health", get(health_handler))
        .route("/mcp/tools/call", post(tool_call_handler))
        .route("/", post(tool_call_handler))
        .with_state(state)
}

async fn health_handler() -> impl IntoResponse {
    Json(serde_json::json!({"status":"ok"}))
}

async fn record_audit(state: &AppState, call: &McpLikeToolCall, decision: &crate::ForwardDecision) {
    if let Some(audit) = &state.audit {
        let mut guard = audit.lock().await;
        // Audit failure must not change the allow/deny decision; best-effort only.
        let _ = guard.append(call, decision);
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
            record_audit(&state, &call, &denied).await;
            let body = serde_json::json!({"error": format!("mediator error: {e}")});
            return (StatusCode::FORBIDDEN, Json(body)).into_response();
        }
    };

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
            let body = serde_json::json!({"error": format!("upstream unreachable: {e}")});
            return (StatusCode::BAD_GATEWAY, Json(body)).into_response();
        }
    };

    let status =
        StatusCode::from_u16(upstream_resp.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
    let bytes = match upstream_resp.bytes().await {
        Ok(b) => b,
        Err(e) => {
            let body = serde_json::json!({"error": format!("upstream read failed: {e}")});
            return (StatusCode::BAD_GATEWAY, Json(body)).into_response();
        }
    };

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
mod tests {
    use super::*;
    use axum::body::Body as AxumBody;
    use axum::http::Request;
    use serde_json::json;
    use tower::ServiceExt;

    use crate::ProxyConfig;

    fn test_mediator_with_upstream(upstream: &str) -> Arc<ProxyMediator> {
        let cfg = ProxyConfig {
            bind: "127.0.0.1:0".to_string(),
            upstream: upstream.to_string(),
            agent_id: "test-agent".to_string(),
            audit_log: None,
        };
        Arc::new(ProxyMediator::new(cfg).expect("mediator"))
    }

    #[tokio::test]
    async fn test_health_returns_ok() {
        let mediator = test_mediator_with_upstream("http://127.0.0.1:9");
        let router = create_router(mediator);
        let req = Request::builder()
            .uri("/health")
            .body(AxumBody::empty())
            .expect("request");
        let resp = router.oneshot(req).await.expect("response");
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_deny_invalid_params_returns_403() {
        let mediator = test_mediator_with_upstream("http://127.0.0.1:9");
        let router = create_router(mediator);
        let body =
            serde_json::to_string(&json!({"tool":"data.read","params":"not a map"})).expect("json");
        let req = Request::builder()
            .uri("/mcp/tools/call")
            .method("POST")
            .header("content-type", "application/json")
            .body(AxumBody::from(body))
            .expect("request");
        let resp = router.oneshot(req).await.expect("response");
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn test_allow_forwards_to_upstream() {
        // Start a tiny upstream mock.
        let upstream_listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind upstream");
        let upstream_addr = upstream_listener.local_addr().expect("addr");
        let upstream_url = format!("http://{upstream_addr}");

        let upstream_router = Router::new().route(
            "/",
            post(|Json(payload): Json<Value>| async move {
                Json(json!({"echo": payload, "upstream": true}))
            }),
        );
        tokio::spawn(async move {
            axum::serve(upstream_listener, upstream_router)
                .await
                .expect("upstream serve");
        });

        let mediator = test_mediator_with_upstream(&upstream_url);
        let router = create_router(mediator);

        let body = serde_json::to_string(&json!({"tool":"data.read","params":{"path":"/tmp/x"}}))
            .expect("json");
        let req = Request::builder()
            .uri("/mcp/tools/call")
            .method("POST")
            .header("content-type", "application/json")
            .body(AxumBody::from(body))
            .expect("request");
        let resp = router.oneshot(req).await.expect("response");
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
            .await
            .expect("bytes");
        let val: Value = serde_json::from_slice(&bytes).expect("json");
        assert_eq!(val["upstream"], json!(true));
        assert_eq!(val["echo"]["tool"], json!("data.read"));
    }

    #[tokio::test]
    async fn test_allow_no_params_forwards() {
        let upstream_listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind");
        let upstream_addr = upstream_listener.local_addr().expect("addr");
        let upstream_url = format!("http://{upstream_addr}");
        let upstream_router = Router::new().route(
            "/",
            post(|Json(payload): Json<Value>| async move {
                Json(json!({"ok": true, "payload": payload}))
            }),
        );
        tokio::spawn(async move {
            axum::serve(upstream_listener, upstream_router)
                .await
                .expect("serve");
        });

        let mediator = test_mediator_with_upstream(&upstream_url);
        let router = create_router(mediator);
        let body = serde_json::to_string(&json!({"tool":"data.read"})).expect("json");
        let req = Request::builder()
            .uri("/")
            .method("POST")
            .header("content-type", "application/json")
            .body(AxumBody::from(body))
            .expect("request");
        let resp = router.oneshot(req).await.expect("response");
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_upstream_unreachable_returns_502() {
        let mediator = test_mediator_with_upstream("http://127.0.0.1:1");
        let router = create_router(mediator);
        let body = serde_json::to_string(&json!({"tool":"data.read","params":{"path":"/tmp/x"}}))
            .expect("json");
        let req = Request::builder()
            .uri("/mcp/tools/call")
            .method("POST")
            .header("content-type", "application/json")
            .body(AxumBody::from(body))
            .expect("request");
        let resp = router.oneshot(req).await.expect("response");
        assert_eq!(resp.status(), StatusCode::BAD_GATEWAY);
    }

    #[tokio::test]
    async fn test_audit_log_records_allow_and_deny() {
        use crate::AuditLog;

        let dir = tempfile::tempdir().expect("tempdir");
        let audit_path = dir.path().join("audit.jsonl");
        let mediator = test_mediator_with_upstream("http://127.0.0.1:9");
        let audit = AuditLog::open(&audit_path).expect("open audit");
        let router = create_router_with_audit(mediator, audit);

        // Allowed call (no params -> stub allows).
        let body = serde_json::to_string(&json!({"tool":"data.read"})).expect("json");
        let req = Request::builder()
            .uri("/mcp/tools/call")
            .method("POST")
            .header("content-type", "application/json")
            .body(AxumBody::from(body))
            .expect("request");
        // Upstream unreachable so status is 502, but audit must still record allow.
        let resp = router.oneshot(req).await.expect("response");
        assert_eq!(resp.status(), StatusCode::BAD_GATEWAY);

        // Denied call (invalid params).
        let mediator2 = test_mediator_with_upstream("http://127.0.0.1:9");
        let audit2 = AuditLog::open(&audit_path).expect("reopen audit");
        let router2 = create_router_with_audit(mediator2, audit2);
        let body2 =
            serde_json::to_string(&json!({"tool":"data.read","params":"not a map"})).expect("json");
        let req2 = Request::builder()
            .uri("/mcp/tools/call")
            .method("POST")
            .header("content-type", "application/json")
            .body(AxumBody::from(body2))
            .expect("request");
        let resp2 = router2.oneshot(req2).await.expect("response");
        assert_eq!(resp2.status(), StatusCode::FORBIDDEN);

        // Verify chain: 2 records, allow then deny.
        AuditLog::verify(&audit_path).expect("verify chain");
        let content = std::fs::read_to_string(&audit_path).expect("read audit");
        let lines: Vec<&str> = content.lines().filter(|l| !l.trim().is_empty()).collect();
        assert_eq!(lines.len(), 2);
        let first: serde_json::Value = serde_json::from_str(lines[0]).expect("parse");
        let second: serde_json::Value = serde_json::from_str(lines[1]).expect("parse");
        assert_eq!(first["decision"], json!("allow"));
        assert_eq!(second["decision"], json!("deny"));
        assert_eq!(second["seq"], json!(2));
    }
}
