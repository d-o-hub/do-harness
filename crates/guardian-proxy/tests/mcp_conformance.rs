//! End-to-end MCP conformance over real Streamable HTTP.
//!
//! Drives the proxy router through a real TCP listener with a raw `reqwest`
//! client (no `rmcp` client dev-dependency) against an in-process upstream.

#![cfg(feature = "mcp-surface")]
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::{Arc, Mutex};

use axum::routing::post;
use axum::{Json, Router};
use guardian_proxy::{AuditLog, ProxyConfig, ProxyMediator, create_router, create_router_degraded};
use serde_json::{Value, json};

const PROTOCOL_VERSION: &str = "2026-07-28";

async fn spawn(router: Router) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let addr = listener.local_addr().expect("addr");
    tokio::spawn(async move {
        axum::serve(listener, router).await.expect("serve");
    });
    format!("http://{addr}")
}

fn mediator(upstream: &str) -> Arc<ProxyMediator> {
    let config = ProxyConfig {
        bind: "127.0.0.1:0".to_string(),
        upstream: upstream.to_string(),
        agent_id: "e2e-agent".to_string(),
        audit_log: None,
        upstream_allowlist: vec![],
        allow_private_upstreams: true,
        metrics_token: None,
        ingress_token: None,
        allowed_origins: vec![],
    };
    Arc::new(ProxyMediator::new(config).expect("mediator"))
}

fn meta() -> Value {
    json!({
        "io.modelcontextprotocol/protocolVersion": PROTOCOL_VERSION,
        "io.modelcontextprotocol/clientInfo": {"name": "e2e", "version": "0.1.0"},
        "io.modelcontextprotocol/clientCapabilities": {}
    })
}

fn call_body(name: &str) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": 42,
        "method": "tools/call",
        "params": {"name": name, "arguments": {"city": "Berlin"}, "_meta": meta()}
    })
}

async fn spawn_mcp_upstream() -> (String, Arc<Mutex<Vec<Value>>>) {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let recorded = Arc::clone(&calls);
    let router = Router::new().route(
        "/",
        post(move |Json(body): Json<Value>| {
            let recorded = Arc::clone(&recorded);
            async move {
                recorded.lock().unwrap().push(body.clone());
                let id = body.get("id").cloned().unwrap_or(json!(1));
                let result = if body.get("method").and_then(Value::as_str) == Some("tools/call") {
                    json!({"content": [{"type": "text", "text": "e2e-ok"}], "isError": false})
                } else {
                    json!({"tools": []})
                };
                Json(json!({"jsonrpc": "2.0", "id": id, "result": result}))
            }
        }),
    );
    (spawn(router).await, calls)
}

fn mcp_post(proxy: &str, body: &Value) -> reqwest::RequestBuilder {
    reqwest::Client::new()
        .post(format!("{proxy}/mcp"))
        .header("content-type", "application/json")
        .header("accept", "application/json, text/event-stream")
        .header("mcp-protocol-version", PROTOCOL_VERSION)
        .header("mcp-method", "tools/call")
        .header("mcp-name", "weather")
        .json(body)
}

#[tokio::test(flavor = "current_thread")]
async fn tool_call_round_trip_records_audit_and_metrics() {
    let (upstream, calls) = spawn_mcp_upstream().await;
    let dir = tempfile::tempdir().expect("tempdir");
    let audit_path = dir.path().join("audit.jsonl");
    let audit = AuditLog::open(&audit_path).expect("audit");
    let proxy = spawn(guardian_proxy::create_router_with_audit(
        mediator(&upstream),
        audit,
    ))
    .await;

    let response = mcp_post(&proxy, &call_body("weather"))
        .send()
        .await
        .expect("send");
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    let value: Value = response.json().await.expect("json");
    assert_eq!(value["id"], json!(42));
    assert_eq!(value["result"]["content"][0]["text"], json!("e2e-ok"));

    {
        let calls = calls.lock().unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0]["params"]["name"], json!("weather"));
    }

    let metrics: Value = reqwest::get(format!("{proxy}/metrics"))
        .await
        .expect("metrics")
        .json()
        .await
        .expect("metrics json");
    assert_eq!(metrics["allow"], json!(1));
    assert_eq!(metrics["upstream_ok"], json!(1));

    AuditLog::verify(&audit_path).expect("intact chain");
    let contents = std::fs::read_to_string(&audit_path).expect("audit file");
    assert!(contents.contains(r#""tool":"weather""#), "{contents}");
    assert!(contents.contains(r#""decision":"allow""#), "{contents}");
}

#[tokio::test(flavor = "current_thread")]
async fn ingress_token_gates_the_mcp_endpoint() {
    let (upstream, calls) = spawn_mcp_upstream().await;
    let config = ProxyConfig {
        bind: "127.0.0.1:0".to_string(),
        upstream: upstream.clone(),
        agent_id: "e2e-agent".to_string(),
        audit_log: None,
        upstream_allowlist: vec![],
        allow_private_upstreams: true,
        metrics_token: None,
        ingress_token: Some("s3cret".to_string()),
        allowed_origins: vec![],
    };
    let mediator = Arc::new(ProxyMediator::new(config).expect("mediator"));
    let mut state = guardian_proxy::AppState::new(mediator);
    state.ingress_token = Some("s3cret".to_string());
    let proxy = spawn(guardian_proxy::create_router_with_state(state)).await;

    let denied = mcp_post(&proxy, &call_body("weather"))
        .send()
        .await
        .expect("send");
    assert_eq!(denied.status(), reqwest::StatusCode::UNAUTHORIZED);
    assert_eq!(
        denied
            .headers()
            .get("www-authenticate")
            .and_then(|value| value.to_str().ok()),
        Some("Bearer realm=\"guardian-proxy\"")
    );
    assert!(calls.lock().unwrap().is_empty());

    let allowed = mcp_post(&proxy, &call_body("weather"))
        .header("authorization", "Bearer s3cret")
        .send()
        .await
        .expect("send");
    assert_eq!(allowed.status(), reqwest::StatusCode::OK);
    let value: Value = allowed.json().await.expect("json");
    assert_eq!(value["result"]["content"][0]["text"], json!("e2e-ok"));
    assert_eq!(calls.lock().unwrap().len(), 1);
}

#[tokio::test(flavor = "current_thread")]
async fn oversized_mcp_body_is_rejected() {
    let (upstream, calls) = spawn_mcp_upstream().await;
    let proxy = spawn(create_router(mediator(&upstream))).await;
    let mut body = call_body("weather");
    body["params"]["arguments"] = json!({"blob": "x".repeat(1024 * 1024 + 64)});

    let response = mcp_post(&proxy, &body).send().await.expect("send");
    assert_eq!(response.status(), reqwest::StatusCode::PAYLOAD_TOO_LARGE);
    assert!(calls.lock().unwrap().is_empty());
}

#[tokio::test(flavor = "current_thread")]
async fn degraded_mcp_call_returns_error_and_skips_upstream() {
    let (_upstream, calls) = spawn_mcp_upstream().await;
    let proxy = spawn(create_router_degraded("mediator init failed".to_string())).await;

    let response = mcp_post(&proxy, &call_body("weather"))
        .send()
        .await
        .expect("send");
    let value: Value = response.json().await.expect("json");
    let message = value["error"]["message"].as_str().unwrap_or_default();
    assert!(message.contains("governance unavailable"), "{message}");
    assert!(calls.lock().unwrap().is_empty());
}
