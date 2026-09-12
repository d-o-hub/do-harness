#![allow(clippy::unwrap_used, clippy::expect_used)]

use axum::body::Body as AxumBody;
use axum::http::{Request, StatusCode};
use serde_json::{Value, json};
use tower::ServiceExt;

use crate::create_router_degraded;

fn discover_request(with_protocol_header: bool) -> Request<AxumBody> {
    let body = serde_json::to_string(&json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "server/discover",
        "params": {
            "_meta": {
                "io.modelcontextprotocol/protocolVersion": "2026-07-28",
                "io.modelcontextprotocol/clientInfo": {"name": "test", "version": "0.1.0"},
                "io.modelcontextprotocol/clientCapabilities": {}
            }
        }
    }))
    .expect("json");
    let mut builder = Request::builder()
        .uri("/mcp")
        .method("POST")
        .header("content-type", "application/json")
        .header("accept", "application/json, text/event-stream")
        .header("host", "localhost")
        .header("mcp-method", "server/discover");
    if with_protocol_header {
        builder = builder.header("mcp-protocol-version", "2026-07-28");
    }
    builder.body(AxumBody::from(body)).expect("request")
}

#[tokio::test(flavor = "current_thread")]
async fn discover_returns_supported_versions() {
    let router = create_router_degraded("test".to_string());
    let resp = router
        .oneshot(discover_request(true))
        .await
        .expect("response");
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
        .await
        .expect("bytes");
    let val: Value = serde_json::from_slice(&bytes).expect("json");
    assert_eq!(val["jsonrpc"], json!("2.0"));
    assert_eq!(val["id"], json!(1));
    assert_eq!(val["result"]["supportedVersions"], json!(["2026-07-28"]));
    assert_eq!(
        val["result"]["_meta"]["io.modelcontextprotocol/serverInfo"]["name"],
        json!("guardian-proxy")
    );
}

#[tokio::test(flavor = "current_thread")]
async fn missing_protocol_version_header_is_rejected() {
    let router = create_router_degraded("test".to_string());
    let resp = router
        .oneshot(discover_request(false))
        .await
        .expect("response");
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test(flavor = "current_thread")]
async fn legacy_tool_call_route_keeps_precedence() {
    let router = create_router_degraded("test".to_string());
    let req = Request::builder()
        .uri("/mcp/tools/call")
        .method("POST")
        .header("content-type", "application/json")
        .body(AxumBody::from(r#"{"tool":"x","params":{}}"#))
        .expect("request");
    let resp = router.oneshot(req).await.expect("response");
    assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
}
