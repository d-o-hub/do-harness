//! In-memory observability counters for the guardian proxy.
//!
//! Lock-free `AtomicU64` counters; snapshot via `GET /metrics` as JSON.
//! Counters only increase and never affect allow/deny decisions.

#![forbid(unsafe_code)]

use std::sync::atomic::{AtomicU64, Ordering};

/// Thread-safe proxy counters.
#[derive(Debug, Default)]
pub struct ProxyMetrics {
    allow: AtomicU64,
    deny: AtomicU64,
    mediator_errors: AtomicU64,
    upstream_ok: AtomicU64,
    upstream_failures: AtomicU64,
    audit_write_failures: AtomicU64,
}

/// JSON snapshot of [`ProxyMetrics`].
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MetricsSnapshot {
    /// Allowed decisions.
    pub allow: u64,
    /// Denied decisions (includes mediator errors).
    pub deny: u64,
    /// `decide()` returned `Err` (fail-closed deny).
    pub mediator_errors: u64,
    /// Forwarded calls with upstream response body read.
    pub upstream_ok: u64,
    /// Upstream send/read failures (returned as 502).
    pub upstream_failures: u64,
    /// Best-effort audit appends that failed.
    pub audit_write_failures: u64,
}

impl ProxyMetrics {
    /// Creates zeroed counters.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Records an allow decision.
    pub fn inc_allow(&self) {
        self.allow.fetch_add(1, Ordering::Relaxed);
    }

    /// Records a deny decision.
    pub fn inc_deny(&self) {
        self.deny.fetch_add(1, Ordering::Relaxed);
    }

    /// Records a mediator error (always paired with a deny).
    pub fn inc_mediator_error(&self) {
        self.mediator_errors.fetch_add(1, Ordering::Relaxed);
    }

    /// Records a successful upstream forward.
    pub fn inc_upstream_ok(&self) {
        self.upstream_ok.fetch_add(1, Ordering::Relaxed);
    }

    /// Records an upstream send/read failure.
    pub fn inc_upstream_failure(&self) {
        self.upstream_failures.fetch_add(1, Ordering::Relaxed);
    }

    /// Records a failed best-effort audit append.
    pub fn inc_audit_write_failure(&self) {
        self.audit_write_failures.fetch_add(1, Ordering::Relaxed);
    }

    /// Returns a point-in-time snapshot.
    #[must_use]
    pub fn snapshot(&self) -> MetricsSnapshot {
        MetricsSnapshot {
            allow: self.allow.load(Ordering::Relaxed),
            deny: self.deny.load(Ordering::Relaxed),
            mediator_errors: self.mediator_errors.load(Ordering::Relaxed),
            upstream_ok: self.upstream_ok.load(Ordering::Relaxed),
            upstream_failures: self.upstream_failures.load(Ordering::Relaxed),
            audit_write_failures: self.audit_write_failures.load(Ordering::Relaxed),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_counters_snapshot() {
        let m = ProxyMetrics::new();
        m.inc_allow();
        m.inc_allow();
        m.inc_deny();
        m.inc_mediator_error();
        m.inc_upstream_ok();
        m.inc_upstream_failure();
        m.inc_audit_write_failure();
        let snap = m.snapshot();
        assert_eq!(
            snap,
            MetricsSnapshot {
                allow: 2,
                deny: 1,
                mediator_errors: 1,
                upstream_ok: 1,
                upstream_failures: 1,
                audit_write_failures: 1,
            }
        );
    }

    #[test]
    fn test_snapshot_starts_zeroed() {
        let snap = ProxyMetrics::new().snapshot();
        assert_eq!(snap.allow, 0);
        assert_eq!(snap.deny, 0);
        assert_eq!(snap.upstream_failures, 0);
        assert_eq!(snap.audit_write_failures, 0);
    }
}
