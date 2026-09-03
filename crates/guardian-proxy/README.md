# guardian-proxy

Optional fail-closed guardian proxy — adjacent to `do-harness` dev harness, not part of it.

- **Off by default:** no `agent-governance` dep unless `agt-governance` feature is enabled.
- **Fail-closed:** `ProxyMediator::decide` returns `Deny` on invalid params or governance denial, never allow on error.
- **Transport v1:** `axum` HTTP proxy (`GET /health`, `POST /mcp/tools/call` + `POST /`) with fail-closed mediation via `ProxyMediator::decide` and `reqwest` forwarding to `ProxyConfig.upstream`; invalid params or governance denial returns `403`, upstream unreachable returns `502`.
- **Audit v1:** optional hash-chained JSONL decision log (`ProxyConfig.audit_log`, `AuditLog::open/append/verify`, `SHA-256(prev|" |payload)`); every `Allow`/`Deny` appended best-effort without changing the decision; tamper detected on reopen via `chain_hash`/`prev_hash` check.
- **Metrics v1:** in-memory counters (`ProxyMetrics`: `allow`, `deny`, `mediator_errors`, `upstream_ok`, `upstream_failures`, `audit_write_failures`) exposed as JSON at `GET /metrics`; counters never affect decisions.

Run:

```bash
cargo check -p guardian-proxy
cargo check -p guardian-proxy --features agt-governance
cargo test -p guardian-proxy
cp crates/guardian-proxy/guardian-proxy.example.toml guardian-proxy.toml
cargo run -p guardian-proxy -- --config guardian-proxy.toml
cargo run -p guardian-proxy -- --verify-audit guardian-audit.jsonl
```

See `plans/agt-governance-epic.md` for promotion criteria.
