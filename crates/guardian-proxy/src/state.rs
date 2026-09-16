//! Shared router state for the guardian proxy.

#![forbid(unsafe_code)]

use std::sync::Arc;
use std::time::Duration;

use axum::http::HeaderMap;

use crate::{AuditLog, ProxyMediator, ProxyMetrics};

/// Upstream request timeout; a hung upstream must not pin a proxy task.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// Shared state for the router.
#[derive(Clone)]
pub struct AppState {
    /// Mediator that decides allow/deny; [`None`] in degraded state, where all
    /// calls are denied.
    pub mediator: Option<Arc<ProxyMediator>>,
    /// Upstream base URL.
    pub upstream: String,
    /// HTTP client for forwarding allowed calls.
    pub client: reqwest::Client,
    /// Optional audit log for decision evidence.
    pub audit: Option<Arc<tokio::sync::Mutex<AuditLog>>>,
    /// In-memory observability counters.
    pub metrics: Arc<ProxyMetrics>,
    /// Governance initialization error, when the mediator could not be built.
    /// Set on degraded state so `/health` fails closed (503) and every call is
    /// denied.
    pub init_error: Option<String>,
    /// Bearer token required on `GET /metrics` when set.
    pub metrics_token: Option<String>,
    /// Bearer token required on the mediation ingress when set.
    pub ingress_token: Option<String>,
    /// Browser origins allowed on the MCP endpoint (RFC 6454 match).
    pub allowed_origins: Vec<String>,
}

impl AppState {
    /// Creates state from a mediator.
    #[must_use]
    pub fn new(mediator: Arc<ProxyMediator>) -> Self {
        let upstream = mediator.upstream().to_string();
        let allowed_origins = mediator.allowed_origins().to_vec();
        Self {
            mediator: Some(mediator),
            upstream,
            client: upstream_client(),
            audit: None,
            metrics: Arc::new(ProxyMetrics::new()),
            init_error: None,
            metrics_token: None,
            ingress_token: None,
            allowed_origins,
        }
    }

    /// Creates state with audit log.
    #[must_use]
    pub fn with_audit(mediator: Arc<ProxyMediator>, audit: AuditLog) -> Self {
        let upstream = mediator.upstream().to_string();
        let allowed_origins = mediator.allowed_origins().to_vec();
        Self {
            mediator: Some(mediator),
            upstream,
            client: upstream_client(),
            audit: Some(Arc::new(tokio::sync::Mutex::new(audit))),
            metrics: Arc::new(ProxyMetrics::new()),
            init_error: None,
            metrics_token: None,
            ingress_token: None,
            allowed_origins,
        }
    }

    /// Creates degraded state: no mediator, all calls denied, health 503.
    #[must_use]
    pub fn degraded(error: String) -> Self {
        Self {
            mediator: None,
            upstream: String::new(),
            client: upstream_client(),
            audit: None,
            metrics: Arc::new(ProxyMetrics::new()),
            init_error: Some(error),
            metrics_token: None,
            ingress_token: None,
            allowed_origins: crate::default_allowed_origins(),
        }
    }

    /// Returns true when the request may use the mediation ingress.
    ///
    /// An unset `ingress_token` disables the check.
    #[must_use]
    pub fn ingress_authorized(&self, headers: &HeaderMap) -> bool {
        token_authorized(self.ingress_token.as_deref(), headers)
    }

    /// Returns true when the request may read `GET /metrics`.
    ///
    /// An unset `metrics_token` disables the check.
    #[must_use]
    pub fn metrics_authorized(&self, headers: &HeaderMap) -> bool {
        token_authorized(self.metrics_token.as_deref(), headers)
    }
}

/// Shared bearer rule: unset disables the check, empty denies every caller.
fn token_authorized(expected: Option<&str>, headers: &HeaderMap) -> bool {
    match expected {
        None => true,
        Some("") => false,
        // Plain comparison mirrors the check this replaced: the proxy is a
        // loopback development sidecar and no timing side channel is claimed.
        Some(expected) => bearer(headers).is_some_and(|presented| presented == expected),
    }
}

/// Extracts the credential from `Authorization: Bearer <token>`.
fn bearer(headers: &HeaderMap) -> Option<&str> {
    headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
}

/// Builds the upstream client with a finite timeout.
///
/// A builder failure (TLS configuration) is reported to stderr; the default
/// client still works, just without the timeout.
fn upstream_client() -> reqwest::Client {
    match reqwest::Client::builder().timeout(REQUEST_TIMEOUT).build() {
        Ok(client) => client,
        Err(err) => {
            eprintln!("guardian-proxy: client build failed ({err}); using default client");
            reqwest::Client::new()
        }
    }
}
