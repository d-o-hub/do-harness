//! Shared router state for the guardian proxy.

#![forbid(unsafe_code)]

use std::sync::Arc;

use crate::{AuditLog, ProxyMediator, ProxyMetrics};

/// Shared state for the router.
#[derive(Clone)]
pub struct AppState {
    /// Mediator that decides allow/deny.
    pub mediator: Arc<ProxyMediator>,
    /// Upstream base URL.
    pub upstream: String,
    /// HTTP client for forwarding allowed calls.
    pub client: reqwest::Client,
    /// Optional audit log for decision evidence.
    pub audit: Option<Arc<tokio::sync::Mutex<AuditLog>>>,
    /// In-memory observability counters.
    pub metrics: Arc<ProxyMetrics>,
}

impl AppState {
    /// Creates state from a mediator.
    #[must_use]
    pub fn new(mediator: Arc<ProxyMediator>) -> Self {
        let upstream = mediator.upstream().to_string();
        let client = reqwest::Client::new();
        Self {
            mediator,
            upstream,
            client,
            audit: None,
            metrics: Arc::new(ProxyMetrics::new()),
        }
    }

    /// Creates state with audit log.
    #[must_use]
    pub fn with_audit(mediator: Arc<ProxyMediator>, audit: AuditLog) -> Self {
        let upstream = mediator.upstream().to_string();
        let client = reqwest::Client::new();
        Self {
            mediator,
            upstream,
            client,
            audit: Some(Arc::new(tokio::sync::Mutex::new(audit))),
            metrics: Arc::new(ProxyMetrics::new()),
        }
    }
}
