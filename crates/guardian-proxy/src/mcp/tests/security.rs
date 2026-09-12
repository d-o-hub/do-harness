//! MCP protocol, origin, and header negative-path coverage.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;

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
async fn origin_not_allowed_is_forbidden() {
    let upstream = spawn_mock_upstream().await;
    let mediator = test_mediator_with_origins(&upstream.url, vec!["http://localhost".to_string()]);
    let router = create_router(mediator);
    let discover = serde_json::to_string(&json!({
        "jsonrpc": "2.0", "id": 1, "method": "server/discover", "params": {"_meta": meta()}
    }))
    .expect("json");

    let allowed = Request::builder()
        .uri("/mcp")
        .method("POST")
        .header("content-type", "application/json")
        .header("accept", "application/json, text/event-stream")
        .header("host", "localhost")
        .header("mcp-method", "server/discover")
        .header("mcp-protocol-version", "2026-07-28")
        .header("origin", "http://localhost")
        .body(AxumBody::from(discover.clone()))
        .expect("request");
    let response = router.clone().oneshot(allowed).await.expect("response");
    assert_eq!(response.status(), StatusCode::OK);

    let denied = Request::builder()
        .uri("/mcp")
        .method("POST")
        .header("content-type", "application/json")
        .header("accept", "application/json, text/event-stream")
        .header("host", "localhost")
        .header("mcp-method", "server/discover")
        .header("mcp-protocol-version", "2026-07-28")
        .header("origin", "http://evil.example")
        .body(AxumBody::from(discover))
        .expect("request");
    let response = router.oneshot(denied).await.expect("response");
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test(flavor = "current_thread")]
async fn unsupported_protocol_version_is_rejected() {
    let router = create_router_degraded("test".to_string());
    let body = serde_json::to_string(&json!({
        "jsonrpc": "2.0", "id": 1, "method": "server/discover",
        "params": {"_meta": {
            "io.modelcontextprotocol/protocolVersion": "1900-01-01",
            "io.modelcontextprotocol/clientInfo": {"name": "test", "version": "0.1.0"},
            "io.modelcontextprotocol/clientCapabilities": {}
        }}
    }))
    .expect("json");
    let request = Request::builder()
        .uri("/mcp")
        .method("POST")
        .header("content-type", "application/json")
        .header("accept", "application/json, text/event-stream")
        .header("host", "localhost")
        .header("mcp-method", "server/discover")
        .header("mcp-protocol-version", "1900-01-01")
        .body(AxumBody::from(body))
        .expect("request");
    let response = router.oneshot(request).await.expect("response");
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let value = body_json(response).await;
    assert_eq!(value["error"]["code"], json!(-32022));
    let supported = value["error"]["data"]["supported"]
        .as_array()
        .expect("supported list");
    assert!(supported.iter().any(|version| version == "2026-07-28"));
}

#[tokio::test(flavor = "current_thread")]
async fn protocol_version_mismatch_between_header_and_body_is_rejected() {
    let router = create_router_degraded("test".to_string());
    let body = serde_json::to_string(&json!({
        "jsonrpc": "2.0", "id": 1, "method": "server/discover",
        "params": {"_meta": {
            "io.modelcontextprotocol/protocolVersion": "2025-11-25",
            "io.modelcontextprotocol/clientInfo": {"name": "test", "version": "0.1.0"},
            "io.modelcontextprotocol/clientCapabilities": {}
        }}
    }))
    .expect("json");
    let request = Request::builder()
        .uri("/mcp")
        .method("POST")
        .header("content-type", "application/json")
        .header("accept", "application/json, text/event-stream")
        .header("host", "localhost")
        .header("mcp-method", "server/discover")
        .header("mcp-protocol-version", "2026-07-28")
        .body(AxumBody::from(body))
        .expect("request");
    let response = router.oneshot(request).await.expect("response");
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let bytes = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("bytes");
    if let Ok(value) = serde_json::from_slice::<Value>(&bytes) {
        assert_eq!(value["error"]["code"], json!(-32020));
    }
}

#[tokio::test(flavor = "current_thread")]
async fn tool_name_header_mismatch_is_rejected() {
    let upstream = spawn_mock_upstream().await;
    let router = create_router(test_mediator(&upstream.url));
    let response = router
        .oneshot(mcp_request(
            "tools/call",
            Some("other"),
            &json!({"name": "echo", "arguments": {}, "_meta": meta()}),
            true,
        ))
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let bytes = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("bytes");
    if let Ok(value) = serde_json::from_slice::<Value>(&bytes) {
        assert_eq!(value["error"]["code"], json!(-32020));
    }
    assert!(upstream.calls.lock().unwrap().is_empty());
}

#[tokio::test(flavor = "current_thread")]
async fn unknown_method_maps_upstream_not_found_to_404() {
    let upstream = spawn_mock_upstream().await;
    let router = create_router(test_mediator(&upstream.url));
    let response = router
        .oneshot(mcp_request(
            "x-vendor/missing",
            None,
            &json!({"_meta": meta()}),
            true,
        ))
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let value = body_json(response).await;
    assert_eq!(value["error"]["code"], json!(-32601));
    let calls = upstream.calls.lock().unwrap();
    assert_eq!(calls.last().unwrap()["method"], json!("x-vendor/missing"));
}

#[tokio::test(flavor = "current_thread")]
async fn notification_is_accepted_with_202() {
    let router = create_router_degraded("test".to_string());
    let body = serde_json::to_string(&json!({
        "jsonrpc": "2.0", "method": "x-vendor/note", "params": {"_meta": meta()}
    }))
    .expect("json");
    let request = Request::builder()
        .uri("/mcp")
        .method("POST")
        .header("content-type", "application/json")
        .header("accept", "application/json, text/event-stream")
        .header("host", "localhost")
        .header("mcp-method", "x-vendor/note")
        .header("mcp-protocol-version", "2026-07-28")
        .body(AxumBody::from(body))
        .expect("request");
    let response = router.oneshot(request).await.expect("response");
    assert_eq!(response.status(), StatusCode::ACCEPTED);
}
