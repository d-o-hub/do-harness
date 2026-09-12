#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::{Arc, Mutex};

use axum::body::Body as AxumBody;
use axum::http::{HeaderMap, Request, StatusCode};
use axum::routing::post;
use axum::{Json, Router};
use serde_json::{Value, json};
use tower::ServiceExt;

use crate::{
    AuditLog, ProxyConfig, ProxyMediator, create_router, create_router_degraded,
    create_router_with_audit, create_router_with_state,
};

struct MockUpstream {
    url: String,
    calls: Arc<Mutex<Vec<Value>>>,
    names: Arc<Mutex<Vec<Option<String>>>>,
}

async fn spawn_mock_upstream() -> MockUpstream {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let names = Arc::new(Mutex::new(Vec::new()));
    let recorded_calls = Arc::clone(&calls);
    let recorded_names = Arc::clone(&names);
    let router = Router::new().route(
        "/",
        post(move |headers: HeaderMap, Json(body): Json<Value>| {
            let calls = Arc::clone(&recorded_calls);
            let names = Arc::clone(&recorded_names);
            async move {
                calls.lock().unwrap().push(body.clone());
                names.lock().unwrap().push(
                    headers
                        .get("mcp-name")
                        .and_then(|value| value.to_str().ok())
                        .map(str::to_string),
                );
                let id = body.get("id").cloned().unwrap_or(json!(1));
                let method = body
                    .get("method")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                let result = match method {
                    "tools/list" => json!({
                        "tools": [{
                            "name": "echo",
                            "description": "echo tool",
                            "inputSchema": {"type": "object", "properties": {}}
                        }]
                    }),
                    "tools/call" => {
                        json!({"content": [{"type": "text", "text": "ok"}], "isError": false})
                    }
                    _ => json!({"echo": body.get("params").cloned().unwrap_or(Value::Null)}),
                };
                Json(json!({"jsonrpc": "2.0", "id": id, "result": result}))
            }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind upstream");
    let addr = listener.local_addr().expect("addr");
    tokio::spawn(async move {
        axum::serve(listener, router).await.expect("upstream serve");
    });
    MockUpstream {
        url: format!("http://{addr}"),
        calls,
        names,
    }
}

fn test_mediator(upstream: &str) -> Arc<ProxyMediator> {
    let config = ProxyConfig {
        bind: "127.0.0.1:0".to_string(),
        upstream: upstream.to_string(),
        agent_id: "test-agent".to_string(),
        audit_log: None,
        upstream_allowlist: vec![],
        allow_private_upstreams: true,
        metrics_token: None,
    };
    Arc::new(ProxyMediator::new(config).expect("mediator"))
}

fn meta() -> Value {
    json!({
        "io.modelcontextprotocol/protocolVersion": "2026-07-28",
        "io.modelcontextprotocol/clientInfo": {"name": "test", "version": "0.1.0"},
        "io.modelcontextprotocol/clientCapabilities": {}
    })
}

fn mcp_request(
    method: &str,
    name: Option<&str>,
    params: &Value,
    with_protocol_header: bool,
) -> Request<AxumBody> {
    let body = json!({"jsonrpc": "2.0", "id": 7, "method": method, "params": params});
    let mut builder = Request::builder()
        .uri("/mcp")
        .method("POST")
        .header("content-type", "application/json")
        .header("accept", "application/json, text/event-stream")
        .header("host", "localhost")
        .header("mcp-method", method);
    if let Some(name) = name {
        builder = builder.header("mcp-name", name);
    }
    if with_protocol_header {
        builder = builder.header("mcp-protocol-version", "2026-07-28");
    }
    builder
        .body(AxumBody::from(body.to_string()))
        .expect("request")
}

async fn body_json(response: axum::response::Response) -> Value {
    let bytes = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("bytes");
    serde_json::from_slice(&bytes).expect("json")
}

#[tokio::test(flavor = "current_thread")]
async fn discover_returns_supported_versions() {
    let router = create_router_degraded("test".to_string());
    let response = router
        .oneshot(mcp_request(
            "server/discover",
            None,
            &json!({"_meta": meta()}),
            true,
        ))
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::OK);
    let value = body_json(response).await;
    assert_eq!(value["jsonrpc"], json!("2.0"));
    assert_eq!(value["result"]["supportedVersions"], json!(["2026-07-28"]));
    assert_eq!(
        value["result"]["_meta"]["io.modelcontextprotocol/serverInfo"]["name"],
        json!("guardian-proxy")
    );
}

#[tokio::test(flavor = "current_thread")]
async fn missing_protocol_version_header_is_rejected() {
    let router = create_router_degraded("test".to_string());
    let response = router
        .oneshot(mcp_request(
            "server/discover",
            None,
            &json!({"_meta": meta()}),
            false,
        ))
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test(flavor = "current_thread")]
async fn legacy_tool_call_route_keeps_precedence() {
    let router = create_router_degraded("test".to_string());
    let request = Request::builder()
        .uri("/mcp/tools/call")
        .method("POST")
        .header("content-type", "application/json")
        .body(AxumBody::from(r#"{"tool":"x","params":{}}"#))
        .expect("request");
    let response = router.oneshot(request).await.expect("response");
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
}

#[tokio::test(flavor = "current_thread")]
async fn list_tools_forwards_to_upstream() {
    let upstream = spawn_mock_upstream().await;
    let router = create_router(test_mediator(&upstream.url));
    let response = router
        .oneshot(mcp_request(
            "tools/list",
            None,
            &json!({"_meta": meta()}),
            true,
        ))
        .await
        .expect("response");
    let value = body_json(response).await;
    assert_eq!(value["result"]["tools"][0]["name"], json!("echo"));
    let calls = upstream.calls.lock().unwrap();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0]["method"], json!("tools/list"));
}

#[tokio::test(flavor = "current_thread")]
async fn call_tool_forwards_after_allow() {
    let upstream = spawn_mock_upstream().await;
    let router = create_router(test_mediator(&upstream.url));
    let metrics_router = router.clone();
    let response = router
        .oneshot(mcp_request(
            "tools/call",
            Some("echo"),
            &json!({"name": "echo", "arguments": {"value": 1}, "_meta": meta()}),
            true,
        ))
        .await
        .expect("response");
    let value = body_json(response).await;
    assert_eq!(value["result"]["content"][0]["text"], json!("ok"));
    assert_eq!(value["result"]["isError"], json!(false));

    {
        let names = upstream.names.lock().unwrap();
        assert_eq!(names.last(), Some(&Some("echo".to_string())));
    }
    {
        let calls = upstream.calls.lock().unwrap();
        assert_eq!(calls.last().unwrap()["method"], json!("tools/call"));
    }

    let metrics = metrics_router
        .oneshot(
            Request::builder()
                .uri("/metrics")
                .body(AxumBody::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(metrics.status(), StatusCode::OK);
    let snapshot = body_json(metrics).await;
    assert_eq!(snapshot["allow"], json!(1));
    assert_eq!(snapshot["deny"], json!(0));
}

#[tokio::test(flavor = "current_thread")]
async fn call_tool_in_degraded_mode_never_reaches_upstream() {
    let upstream = spawn_mock_upstream().await;
    let mut state = crate::AppState::new(test_mediator(&upstream.url));
    state.init_error = Some("mediator init failed".to_string());
    let router = create_router_with_state(state);
    let response = router
        .oneshot(mcp_request(
            "tools/call",
            Some("echo"),
            &json!({"name": "echo", "arguments": {}, "_meta": meta()}),
            true,
        ))
        .await
        .expect("response");
    let value = body_json(response).await;
    let message = value["error"]["message"].as_str().unwrap_or_default();
    assert!(message.contains("governance unavailable"), "{message}");
    assert!(upstream.calls.lock().unwrap().is_empty());
}

#[tokio::test(flavor = "current_thread")]
async fn custom_method_is_passed_through() {
    let upstream = spawn_mock_upstream().await;
    let router = create_router(test_mediator(&upstream.url));
    let response = router
        .oneshot(mcp_request(
            "x-vendor/echo",
            None,
            &json!({"n": 1, "_meta": meta()}),
            true,
        ))
        .await
        .expect("response");
    let value = body_json(response).await;
    assert_eq!(value["result"]["echo"]["n"], json!(1));
}

#[tokio::test(flavor = "current_thread")]
async fn audit_records_allowed_mcp_call() {
    let upstream = spawn_mock_upstream().await;
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("audit.jsonl");
    let audit = AuditLog::open(&path).expect("audit");
    let router = create_router_with_audit(test_mediator(&upstream.url), audit);
    let response = router
        .oneshot(mcp_request(
            "tools/call",
            Some("echo"),
            &json!({"name": "echo", "arguments": {}, "_meta": meta()}),
            true,
        ))
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::OK);
    AuditLog::verify(&path).expect("intact chain");
    let contents = std::fs::read_to_string(&path).expect("audit file");
    assert!(contents.contains(r#""tool":"echo""#), "{contents}");
    assert!(contents.contains(r#""decision":"allow""#), "{contents}");
}

#[tokio::test(flavor = "current_thread")]
async fn upstream_unreachable_fails_closed() {
    let router = create_router(test_mediator("http://127.0.0.1:9"));
    let response = router
        .oneshot(mcp_request(
            "tools/call",
            Some("echo"),
            &json!({"name": "echo", "arguments": {}, "_meta": meta()}),
            true,
        ))
        .await
        .expect("response");
    let value = body_json(response).await;
    let message = value["error"]["message"].as_str().unwrap_or_default();
    assert!(message.contains("upstream unreachable"), "{message}");
}
