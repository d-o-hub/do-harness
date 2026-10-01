//! MCP protocol, origin, and header negative-path coverage.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use axum::http::{HeaderName, HeaderValue, header::ORIGIN};

use super::*;

/// A valid `tools/call` for the mock upstream's `echo` tool.
fn tool_call_request() -> Request<AxumBody> {
    mcp_request(
        "tools/call",
        Some("echo"),
        &json!({"name": "echo", "arguments": {}, "_meta": meta()}),
        true,
    )
}

/// Replaces the single value of `name` with `value`.
fn set_header(request: Request<AxumBody>, name: &'static str, value: &str) -> Request<AxumBody> {
    let mut request = request;
    request.headers_mut().insert(
        HeaderName::from_static(name),
        HeaderValue::from_str(value).expect("header value"),
    );
    request
}

/// Appends a second value for `name`: RFC 9110 list semantics do not apply to
/// these routing headers, so the request now carries a duplicate.
fn append_header(request: Request<AxumBody>, name: &'static str, value: &str) -> Request<AxumBody> {
    let mut request = request;
    request.headers_mut().append(
        HeaderName::from_static(name),
        HeaderValue::from_str(value).expect("header value"),
    );
    request
}

/// Adds an `Origin` header.
fn with_origin(request: Request<AxumBody>, origin: &str) -> Request<AxumBody> {
    let mut request = request;
    request
        .headers_mut()
        .insert(ORIGIN, HeaderValue::from_str(origin).expect("header value"));
    request
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
async fn explicit_default_origin_port_is_normalized() {
    let upstream = spawn_mock_upstream().await;
    let mediator =
        test_mediator_with_origins(&upstream.url, vec!["http://localhost:80".to_string()]);
    let router = create_router(mediator);

    // An `Origin` without a port means the scheme default (`http` -> 80), so an
    // allowlisted explicit port must match both spellings. The port is explicit
    // on purpose: a portless allowlist entry still matches any port in this SDK
    // release (deprecated upstream behavior) and would not pin this boundary.
    for origin in ["http://localhost", "http://localhost:80"] {
        let response = router
            .clone()
            .oneshot(with_origin(tool_call_request(), origin))
            .await
            .expect("response");
        assert_eq!(
            response.status(),
            StatusCode::OK,
            "{origin} must be allowed"
        );
        let value = body_json(response).await;
        assert_eq!(value["result"]["content"][0]["text"], json!("ok"));
    }
    assert_eq!(
        upstream.calls.lock().len(),
        2,
        "allowed calls must reach the upstream"
    );

    let response = router
        .oneshot(with_origin(tool_call_request(), "http://localhost:81"))
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    assert_eq!(
        upstream.calls.lock().len(),
        2,
        "a rejected origin must not reach the upstream"
    );
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
    assert!(upstream.calls.lock().is_empty());
}

#[tokio::test(flavor = "current_thread")]
async fn duplicate_routing_headers_are_rejected_before_forwarding() {
    let upstream = spawn_mock_upstream().await;
    let router = create_router(test_mediator(&upstream.url));
    // `Mcp-Method`/`Mcp-Name` are singletons: a duplicate must fail regardless
    // of order, so each header is exercised with a matching first value (which
    // rules out duplicate detection being an accident of mismatch handling), a
    // reversed pair, and two identical values.
    let cases = [
        ("mcp-method", "tools/call", "tools/list"),
        ("mcp-method", "tools/list", "tools/call"),
        ("mcp-method", "tools/call", "tools/call"),
        ("mcp-name", "echo", "other"),
        ("mcp-name", "other", "echo"),
        ("mcp-name", "echo", "echo"),
    ];
    for (header, first, second) in cases {
        let request = append_header(
            set_header(tool_call_request(), header, first),
            header,
            second,
        );
        let response = router.clone().oneshot(request).await.expect("response");
        assert_eq!(
            response.status(),
            StatusCode::BAD_REQUEST,
            "{header} {first}/{second} must be rejected"
        );
        let value = body_json(response).await;
        assert_eq!(
            value["error"]["code"],
            json!(-32020),
            "{header} {first}/{second} must map to a header mismatch"
        );
    }
    assert!(
        upstream.calls.lock().is_empty(),
        "rejected requests must never reach the upstream"
    );
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
    let calls = upstream.calls.lock();
    assert_eq!(calls.last().unwrap()["method"], json!("x-vendor/missing"));
}

#[tokio::test(flavor = "current_thread")]
async fn mcp_ingress_requires_bearer_token() {
    let upstream = spawn_mock_upstream().await;
    let router = create_router_with_state(state_with_ingress_token(&upstream.url, Some("s3cret")));

    let denied = router
        .clone()
        .oneshot(mcp_request(
            "tools/list",
            None,
            &json!({"_meta": meta()}),
            true,
        ))
        .await
        .expect("response");
    assert_eq!(denied.status(), StatusCode::UNAUTHORIZED);
    assert!(upstream.calls.lock().is_empty());

    let allowed = router
        .oneshot(with_bearer(
            mcp_request("tools/list", None, &json!({"_meta": meta()}), true),
            "s3cret",
        ))
        .await
        .expect("response");
    assert_eq!(allowed.status(), StatusCode::OK);
    let calls = upstream.calls.lock();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0]["method"], json!("tools/list"));
}

#[tokio::test(flavor = "current_thread")]
async fn mcp_discover_requires_bearer_token() {
    let upstream = spawn_mock_upstream().await;
    let router = create_router_with_state(state_with_ingress_token(&upstream.url, Some("s3cret")));

    // `server/discover` is answered by the transport itself, not our handler,
    // so this proves the layer wraps the nested service.
    let denied = router
        .clone()
        .oneshot(mcp_request(
            "server/discover",
            None,
            &json!({"_meta": meta()}),
            true,
        ))
        .await
        .expect("response");
    assert_eq!(denied.status(), StatusCode::UNAUTHORIZED);

    let allowed = router
        .oneshot(with_bearer(
            mcp_request("server/discover", None, &json!({"_meta": meta()}), true),
            "s3cret",
        ))
        .await
        .expect("response");
    assert_eq!(allowed.status(), StatusCode::OK);
    let value = body_json(allowed).await;
    assert_eq!(value["result"]["supportedVersions"], json!(["2026-07-28"]));
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
