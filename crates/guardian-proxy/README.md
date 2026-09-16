# guardian-proxy

Optional fail-closed guardian proxy — adjacent to `do-harness` dev harness, not part of it.

- **Off by default:** no `agent-governance` dep unless `agt-governance` is enabled; no `rmcp` dep unless `mcp-surface` is enabled.
- **Fail-closed:** `ProxyMediator::decide` returns `Deny` on invalid params or governance denial, never allow on error.
- **MCP surface (`mcp-surface`, off by default):** `POST /mcp` is a Streamable HTTP MCP ingress for protocol `2026-07-28` built on the official `rmcp` SDK. It serves `server/discover`, forwards `tools/list`, mediates `tools/call` (decide → audit → metrics → forward only on allow) and passes other methods through. Stateless with JSON responses; requires `MCP-Protocol-Version`/`Mcp-Method`/`Mcp-Name` headers; `Host` is restricted to loopback and `Origin` is validated against `allowed_origins` (loopback default); bodies are capped at 1 MiB; modern protocol only (no sessions or `initialize`).
- **Transport v1 (deprecated):** `axum` HTTP proxy (`GET /health`, `GET /metrics`, `POST /mcp/tools/call`) with fail-closed mediation via `ProxyMediator::decide` and `reqwest` forwarding to `ProxyConfig.upstream`; invalid params or governance denial returns `403`, upstream unreachable returns `502`. There is no `POST /` alias. Request bodies and buffered upstream responses are capped at 1 MiB, and upstream requests carry a 30s timeout. The flat tool-call route is retired with `chore-mcp-promotion`: responses carry `Deprecation: true` and, with `mcp-surface`, `Link: </mcp>; rel="successor-version"`.
- **Ingress auth (opt-in):** when `ProxyConfig.ingress_token` is set, `POST /mcp` and `POST /mcp/tools/call` require `Authorization: Bearer <token>`; other credentials get `401` with a `WWW-Authenticate: Bearer` challenge, before mediation, audit, or upstream contact. Unset leaves the ingress open (the loopback-sidecar default). `GET /health` stays unauthenticated and `GET /metrics` keeps its own `metrics_token`; both tokens survive a degraded startup.
- **SSRF guard:** upstreams are validated at startup — link-local/metadata (`169.254.0.0/16`, `metadata.google.internal`) are always rejected; loopback/private hosts need `allow_private_upstreams = true`; a non-empty `upstream_allowlist` must contain the host.
- **Degraded mode:** if mediator initialization fails the process serves a degraded router instead of exiting: `/health` returns `503` and every tool call is denied.
- **Audit v1:** optional hash-chained JSONL decision log (`ProxyConfig.audit_log`, `AuditLog::open/append/verify`, `SHA-256(prev|" |payload)`); every `Allow`/`Deny` appended best-effort without changing the decision; appends take an exclusive file lock and `fsync`, so concurrent proxy processes share one chain; tamper detected on reopen via `chain_hash`/`prev_hash` check; append failures raise a stderr alert.
- **Metrics v1:** in-memory counters (`ProxyMetrics`: `allow`, `deny`, `mediator_errors`, `upstream_ok`, `upstream_failures`, `audit_write_failures`, `stub_decisions`) exposed as JSON at `GET /metrics` (bearer token required when `metrics_token` is set); counters never affect decisions. Every tool-call response carries `X-Do-Harness-Governance: enforced|stub`.

Run:

```bash
cargo check -p guardian-proxy
cargo check -p guardian-proxy --features agt-governance
cargo test -p guardian-proxy
cargo test -p guardian-proxy --features mcp-surface
cp crates/guardian-proxy/guardian-proxy.example.toml guardian-proxy.toml
cargo run -p guardian-proxy -- --config guardian-proxy.toml
cargo run -p guardian-proxy -- --verify-audit guardian-audit.jsonl
```

See `plans/agt-governance-epic.md` for promotion criteria and
`plans/mcp-conformance-epic.md` for the MCP conformance decision and slices.
