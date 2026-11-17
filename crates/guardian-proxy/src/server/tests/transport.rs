//! Transport v1 deprecation and removal-gate coverage.
//!
//! Kept out of `server/tests.rs` so that file stays under the 500-line ceiling.

use super::*;

/// The removal gate is real: with transport v1 disabled the flat route is
/// absent while the observability and MCP surfaces stay mounted.
#[tokio::test(flavor = "current_thread")]
async fn transport_v1_removal_gate_stops_serving_the_legacy_handler() {
    let mediator = test_mediator_with_upstream("http://127.0.0.1:9");
    let router = create_router_with_transport(AppState::new(mediator), false);

    let body = serde_json::to_string(&json!({"tool":"data.read","params":{}})).expect("json");
    let req = Request::builder()
        .uri("/mcp/tools/call")
        .method("POST")
        .header("content-type", "application/json")
        .body(AxumBody::from(body))
        .expect("request");
    let resp = router.oneshot(req).await.expect("response");
    // Without the legacy route the request either falls through to the nested
    // MCP service (which answers a JSON-RPC parse error) or has no route at
    // all. Either way the legacy tag must be absent: that header is written by
    // the v1 handler only.
    #[cfg(feature = "mcp-surface")]
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    #[cfg(not(feature = "mcp-surface"))]
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    assert!(
        resp.headers().get(DEPRECATION_HEADER).is_none(),
        "the v1 handler ran although the removal gate is enabled"
    );

    let health = create_router_with_transport(
        AppState::new(test_mediator_with_upstream("http://127.0.0.1:9")),
        false,
    );
    let req = Request::builder()
        .uri("/health")
        .body(AxumBody::empty())
        .expect("request");
    let resp = health.oneshot(req).await.expect("response");
    assert_eq!(resp.status(), StatusCode::OK, "observability stays mounted");
}

/// Every legacy response advertises the removal target a client must plan for.
#[tokio::test(flavor = "current_thread")]
async fn transport_v1_responses_name_the_removal_target() {
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
    assert_eq!(
        resp.headers()
            .get(DEPRECATION_HEADER)
            .and_then(|value| value.to_str().ok()),
        Some("true")
    );
    assert_eq!(crate::TRANSPORT_V1_REMOVAL_TARGET, "1.0.0");
}
