#![allow(clippy::unwrap_used, clippy::expect_used)]

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
        upstream_allowlist: vec![],
        allow_private_upstreams: true,
        metrics_token: None,
    };
    Arc::new(ProxyMediator::new(cfg).expect("mediator"))
}

#[tokio::test(flavor = "current_thread")]
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

#[tokio::test(flavor = "current_thread")]
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

#[tokio::test(flavor = "current_thread")]
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

#[tokio::test(flavor = "current_thread")]
async fn test_allow_no_params_forwards() {
    let upstream_listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let upstream_addr = upstream_listener.local_addr().expect("addr");
    let upstream_url = format!("http://{upstream_addr}");
    let upstream_router =
        Router::new().route(
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
        .uri("/mcp/tools/call")
        .method("POST")
        .header("content-type", "application/json")
        .body(AxumBody::from(body))
        .expect("request");
    let resp = router.oneshot(req).await.expect("response");
    assert_eq!(resp.status(), StatusCode::OK);
}

/// The removed `POST /` compatibility alias is gone: one entry point only.
#[tokio::test(flavor = "current_thread")]
async fn test_root_alias_is_not_routed() {
    let mediator = test_mediator_with_upstream("http://127.0.0.1:9");
    let router = create_router(mediator);
    let req = Request::builder()
        .uri("/")
        .method("POST")
        .header("content-type", "application/json")
        .body(AxumBody::from("{}"))
        .expect("request");
    let resp = router.oneshot(req).await.expect("response");
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test(flavor = "current_thread")]
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

#[tokio::test(flavor = "current_thread")]
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

#[tokio::test(flavor = "current_thread")]
async fn test_metrics_counts_allow_deny_and_upstream_failure() {
    use crate::ProxyMetrics;

    let mediator = test_mediator_with_upstream("http://127.0.0.1:1");
    let state = AppState {
        metrics: Arc::new(ProxyMetrics::new()),
        ..AppState::new(mediator)
    };
    let metrics = Arc::clone(&state.metrics);
    let router = create_router_with_state(state);

    // Denied call (invalid params).
    let deny_body =
        serde_json::to_string(&json!({"tool":"data.read","params":"not a map"})).expect("json");
    let req = Request::builder()
        .uri("/mcp/tools/call")
        .method("POST")
        .header("content-type", "application/json")
        .body(AxumBody::from(deny_body))
        .expect("request");
    let resp = router.oneshot(req).await.expect("response");
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);

    // Allowed call with unreachable upstream -> 502 + upstream failure.
    let mediator2 = test_mediator_with_upstream("http://127.0.0.1:1");
    let state2 = AppState {
        mediator: Some(mediator2),
        upstream: "http://127.0.0.1:1".to_string(),
        client: reqwest::Client::new(),
        audit: None,
        metrics: Arc::clone(&metrics),
        init_error: None,
        metrics_token: None,
    };
    let router2 = create_router_with_state(state2);
    let allow_body = serde_json::to_string(&json!({"tool":"data.read","params":{"path":"/tmp/x"}}))
        .expect("json");
    let req2 = Request::builder()
        .uri("/mcp/tools/call")
        .method("POST")
        .header("content-type", "application/json")
        .body(AxumBody::from(allow_body))
        .expect("request");
    let resp2 = router2.oneshot(req2).await.expect("response");
    assert_eq!(resp2.status(), StatusCode::BAD_GATEWAY);

    let snap = metrics.snapshot();
    assert_eq!(snap.deny, 1);
    assert_eq!(snap.allow, 1);
    assert_eq!(snap.upstream_failures, 1);
    assert_eq!(snap.audit_write_failures, 0);
}

#[tokio::test(flavor = "current_thread")]
async fn test_metrics_endpoint_returns_snapshot() {
    let mediator = test_mediator_with_upstream("http://127.0.0.1:9");
    let router = create_router(mediator);
    let req = Request::builder()
        .uri("/metrics")
        .body(AxumBody::empty())
        .expect("request");
    let resp = router.oneshot(req).await.expect("response");
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
        .await
        .expect("bytes");
    let val: Value = serde_json::from_slice(&bytes).expect("json");
    for key in [
        "allow",
        "deny",
        "mediator_errors",
        "upstream_ok",
        "upstream_failures",
        "audit_write_failures",
    ] {
        assert!(val.get(key).is_some(), "missing metrics key {key}");
    }
}

#[tokio::test(flavor = "current_thread")]
async fn test_metrics_requires_bearer_token_when_configured() {
    let mediator = test_mediator_with_upstream("http://127.0.0.1:9");
    let mut state = AppState::new(mediator);
    state.metrics_token = Some("secret".to_string());
    let router = create_router_with_state(state);

    let req = Request::builder()
        .uri("/metrics")
        .body(AxumBody::empty())
        .expect("request");
    let resp = router.clone().oneshot(req).await.expect("response");
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);

    let req = Request::builder()
        .uri("/metrics")
        .header("authorization", "Bearer wrong")
        .body(AxumBody::empty())
        .expect("request");
    let resp = router.clone().oneshot(req).await.expect("response");
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);

    let req = Request::builder()
        .uri("/metrics")
        .header("authorization", "Bearer secret")
        .body(AxumBody::empty())
        .expect("request");
    let resp = router.oneshot(req).await.expect("response");
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test(flavor = "current_thread")]
async fn test_degraded_router_fails_closed() {
    let router = create_router_degraded("mediator init failed".to_string());

    let req = Request::builder()
        .uri("/health")
        .body(AxumBody::empty())
        .expect("request");
    let resp = router.clone().oneshot(req).await.expect("response");
    assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);

    let req = Request::builder()
        .uri("/mcp/tools/call")
        .method("POST")
        .header("content-type", "application/json")
        .body(AxumBody::from(r#"{"tool":"data.read"}"#))
        .expect("request");
    let resp = router.oneshot(req).await.expect("response");
    assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
}

#[tokio::test(flavor = "current_thread")]
async fn test_request_body_limit_rejects_oversized_payload() {
    let mediator = test_mediator_with_upstream("http://127.0.0.1:9");
    let router = create_router(mediator);
    let blob = "x".repeat(1024 * 1024 + 1);
    let body = format!(r#"{{"tool":"data.read","params":{{"blob":"{blob}"}}}}"#);
    let req = Request::builder()
        .uri("/mcp/tools/call")
        .method("POST")
        .header("content-type", "application/json")
        .body(AxumBody::from(body))
        .expect("request");
    let resp = router.oneshot(req).await.expect("response");
    assert_eq!(resp.status(), StatusCode::PAYLOAD_TOO_LARGE);
}

#[tokio::test(flavor = "current_thread")]
async fn test_responses_advertise_governance_mode() {
    let mediator = test_mediator_with_upstream("http://127.0.0.1:9");
    let router = create_router(mediator);
    let req = Request::builder()
        .uri("/mcp/tools/call")
        .method("POST")
        .header("content-type", "application/json")
        .body(AxumBody::from(
            r#"{"tool":"data.read","params":"not a map"}"#,
        ))
        .expect("request");
    let resp = router.oneshot(req).await.expect("response");
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    let mode = resp
        .headers()
        .get("x-do-harness-governance")
        .and_then(|value| value.to_str().ok())
        .expect("governance header");
    let expected = if cfg!(feature = "agt-governance") {
        "enforced"
    } else {
        "stub"
    };
    assert_eq!(mode, expected);
}
