//! Shared router state for the guardian proxy.

#![forbid(unsafe_code)]

use std::sync::Arc;
use std::time::Duration;

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
}

impl AppState {
    /// Creates state from a mediator.
    #[must_use]
    pub fn new(mediator: Arc<ProxyMediator>) -> Self {
        let upstream = mediator.upstream().to_string();
        Self {
            mediator: Some(mediator),
            upstream,
            client: upstream_client(),
            audit: None,
            metrics: Arc::new(ProxyMetrics::new()),
            init_error: None,
            metrics_token: None,
        }
    }

    /// Creates state with audit log.
    #[must_use]
    pub fn with_audit(mediator: Arc<ProxyMediator>, audit: AuditLog) -> Self {
        let upstream = mediator.upstream().to_string();
        Self {
            mediator: Some(mediator),
            upstream,
            client: upstream_client(),
            audit: Some(Arc::new(tokio::sync::Mutex::new(audit))),
            metrics: Arc::new(ProxyMetrics::new()),
            init_error: None,
            metrics_token: None,
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
        }
    }
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
