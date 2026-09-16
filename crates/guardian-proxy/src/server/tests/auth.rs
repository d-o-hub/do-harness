//! Ingress bearer-token enforcement on the mediation routes.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;

/// Mediator pointed at `upstream`; mirrors the shared test helper.
fn mediator_for(upstream: &str) -> Arc<ProxyMediator> {
    test_mediator_with_upstream(upstream)
}

/// State whose ingress token is `token` (`None` leaves the ingress open).
fn state_with_token(upstream: &str, token: Option<&str>) -> AppState {
    let mut state = AppState::new(mediator_for(upstream));
    state.ingress_token = token.map(str::to_string);
    state
}

/// Upstream that echoes the request body, so "forwarded" is observable.
async fn spawn_echo_upstream() -> (String, Arc<std::sync::atomic::AtomicUsize>) {
    let calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let recorded = Arc::clone(&calls);
    let router = Router::new().route(
        "/",
        post(move |Json(payload): Json<Value>| {
            let recorded = Arc::clone(&recorded);
            async move {
                recorded.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                Json(json!({"echo": payload, "upstream": true}))
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
    (format!("http://{addr}"), calls)
}

/// `POST /mcp/tools/call` with `body`, optionally carrying `Bearer <token>`.
fn flat_request_with_body(token: Option<&str>, body: &Value) -> Request<AxumBody> {
    let mut builder = Request::builder()
        .uri("/mcp/tools/call")
        .method("POST")
        .header("content-type", "application/json");
    if let Some(token) = token {
        builder = builder.header("authorization", format!("Bearer {token}"));
    }
    builder
        .body(AxumBody::from(body.to_string()))
        .expect("request")
}

/// Well-formed tool call with an optional credential.
fn flat_request(token: Option<&str>) -> Request<AxumBody> {
    flat_request_with_body(
        token,
        &json!({"tool":"data.read","params":{"path":"/tmp/x"}}),
    )
}

#[tokio::test(flavor = "current_thread")]
async fn ingress_token_required_on_flat_route() {
    let (upstream, _calls) = spawn_echo_upstream().await;
    let router = create_router_with_state(state_with_token(&upstream, Some("s3cret")));

    let denied = router
        .clone()
        .oneshot(flat_request(None))
        .await
        .expect("response");
    assert_eq!(denied.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(
        denied
            .headers()
            .get("www-authenticate")
            .and_then(|value| value.to_str().ok()),
        Some("Bearer realm=\"guardian-proxy\"")
    );

    let wrong = router
        .clone()
        .oneshot(flat_request(Some("wrong")))
        .await
        .expect("response");
    assert_eq!(wrong.status(), StatusCode::UNAUTHORIZED);

    let allowed = router
        .oneshot(flat_request(Some("s3cret")))
        .await
        .expect("response");
    assert_eq!(allowed.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(allowed.into_body(), 1024 * 1024)
        .await
        .expect("bytes");
    let value: Value = serde_json::from_slice(&bytes).expect("json");
    assert_eq!(value["echo"]["tool"], json!("data.read"));
}

#[tokio::test(flavor = "current_thread")]
async fn ingress_rejection_precedes_mediation() {
    let (upstream, calls) = spawn_echo_upstream().await;
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("audit.jsonl");
    let audit = AuditLog::open(&path).expect("audit");

    let mut state = AppState::with_audit(mediator_for(&upstream), audit);
    state.ingress_token = Some("s3cret".to_string());
    let metrics = Arc::clone(&state.metrics);
    let router = create_router_with_state(state);

    // The stub mediator denies this body shape with 403; a 401 proves the
    // credential gate ran first.
    let response = router
        .oneshot(flat_request_with_body(
            None,
            &json!({"tool":"data.read","params":"not a map"}),
        ))
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    let snapshot = metrics.snapshot();
    assert_eq!(snapshot.deny, 0);
    assert_eq!(snapshot.allow, 0);
    let audited = std::fs::metadata(&path).map_or(0, |meta| meta.len());
    assert_eq!(audited, 0, "unauthorized requests must not be audited");
    assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 0);
}

#[tokio::test(flavor = "current_thread")]
async fn ingress_open_when_token_unset() {
    let (upstream, _calls) = spawn_echo_upstream().await;
    let router = create_router_with_state(state_with_token(&upstream, None));
    let response = router.oneshot(flat_request(None)).await.expect("response");
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test(flavor = "current_thread")]
async fn observability_routes_ignore_ingress_token() {
    let router = create_router_with_state(state_with_token("http://127.0.0.1:9", Some("s3cret")));

    let health = router
        .clone()
        .oneshot(
            Request::builder()
                .uri("/health")
                .body(AxumBody::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(health.status(), StatusCode::OK);

    let metrics = router
        .oneshot(
            Request::builder()
                .uri("/metrics")
                .body(AxumBody::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(metrics.status(), StatusCode::OK);
}

#[tokio::test(flavor = "current_thread")]
async fn degraded_router_keeps_configured_tokens() {
    let mut state = AppState::degraded("mediator init failed".to_string());
    state.ingress_token = Some("s3cret".to_string());
    state.metrics_token = Some("mtok".to_string());
    let router = create_router_with_state(state);

    let health = router
        .clone()
        .oneshot(
            Request::builder()
                .uri("/health")
                .body(AxumBody::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(health.status(), StatusCode::SERVICE_UNAVAILABLE);

    let metrics_denied = router
        .clone()
        .oneshot(
            Request::builder()
                .uri("/metrics")
                .body(AxumBody::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(metrics_denied.status(), StatusCode::UNAUTHORIZED);

    let metrics_allowed = router
        .clone()
        .oneshot(
            Request::builder()
                .uri("/metrics")
                .header("authorization", "Bearer mtok")
                .body(AxumBody::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(metrics_allowed.status(), StatusCode::OK);

    let flat_denied = router
        .clone()
        .oneshot(flat_request(None))
        .await
        .expect("response");
    assert_eq!(flat_denied.status(), StatusCode::UNAUTHORIZED);

    let flat_allowed = router
        .oneshot(flat_request(Some("s3cret")))
        .await
        .expect("response");
    assert_eq!(flat_allowed.status(), StatusCode::SERVICE_UNAVAILABLE);
}
