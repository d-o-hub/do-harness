# guardian-proxy

Optional fail-closed guardian proxy — adjacent to `do-harness` dev harness, not part of it.

- **Off by default:** no `agent-governance` dep unless `agt-governance` is enabled; no `rmcp` dep unless `mcp-surface` is enabled.
- **Fail-closed:** `ProxyMediator::decide` returns `Deny` on invalid params or governance denial, never allow on error.
- **Declarative policy:** optional versioned YAML policy file (`ProxyConfig.policy_file`) loaded at startup — allow/deny/ask rules over tool name, caller identity, and parameter matchers. Loaded policies fail closed (degraded mode) on parse errors or a missing file; unset keeps the code-embedded defaults. A `policy test --fixtures <dir>` subcommand evaluates allow/deny/ask cases offline. Policy decisions are bound to the audit chain by SHA-256 digest.
- **MCP surface (`mcp-surface`, off by default):** `POST /mcp` is a Streamable HTTP MCP ingress for protocol `2026-07-28` built on the official `rmcp` SDK. It serves `server/discover`, forwards `tools/list`, mediates `tools/call` (decide → audit → metrics → forward only on allow) and passes other methods through. Stateless with JSON responses; requires `MCP-Protocol-Version`/`Mcp-Method`/`Mcp-Name` headers; `Host` is restricted to loopback and `Origin` is validated against `allowed_origins` (loopback default); bodies are capped at 1 MiB; modern protocol only (no sessions or `initialize`).
- **Transport v1 (deprecated):** `axum` HTTP proxy (`GET /health`, `GET /metrics`, `POST /mcp/tools/call`) with fail-closed mediation via `ProxyMediator::decide` and `reqwest` forwarding to `ProxyConfig.upstream`; invalid params or governance denial returns `403`, upstream unreachable returns `502`. There is no `POST /` alias. Request bodies and buffered upstream responses are capped at 1 MiB, and upstream requests carry a 30s timeout. The flat tool-call route is retired with `chore-mcp-promotion`: responses carry `Deprecation: true` and, with `mcp-surface`, `Link: </mcp>; rel="successor-version"`.
- **Ingress auth (opt-in):** when `ProxyConfig.ingress_token` is set, `POST /mcp` and `POST /mcp/tools/call` require `Authorization: Bearer <token>`; other credentials get `401` with a `WWW-Authenticate: Bearer` challenge, before mediation, audit, or upstream contact. Unset leaves the ingress open (the loopback-sidecar default). `GET /health` stays unauthenticated and `GET /metrics` keeps its own `metrics_token`; both tokens survive a degraded startup.
- **SSRF guard:** upstreams are validated at startup — link-local/metadata (`169.254.0.0/16`, `metadata.google.internal`, IPv6 `fe80::/10`) are always rejected; loopback/private hosts need `allow_private_upstreams = true`; a non-empty `upstream_allowlist` must contain the host.
- **Degraded mode:** if mediator initialization fails the process serves a degraded router instead of exiting: `/health` returns `503` and every tool call is denied.
- **Governance mode reporting:** `GET /health` carries the compiled governance mode in its `governance` field — `enforced` with `agt-governance`, `stub` without — alongside `status`, matching the `X-Do-Harness-Governance` header on tool-call responses. A default build therefore never reads as enforcement, including while degraded.
- **Audit v1:** optional hash-chained JSONL decision log (`ProxyConfig.audit_log`, `AuditLog::open/append/verify`, `SHA-256(prev|" |payload)`); every `Allow`/`Deny` appended best-effort without changing the decision; when a declarative policy is loaded its SHA-256 digest is recorded on every record (`policy_sha256`) and bound into the chain hash, so an audit ledger proves which policy produced each decision; appends take an exclusive file lock and `fsync`, so concurrent proxy processes share one chain; tamper detected on reopen via `chain_hash`/`prev_hash` check; append failures raise a stderr alert.
- **Metrics v1:** in-memory counters (`ProxyMetrics`: `allow`, `deny`, `mediator_errors`, `upstream_ok`, `upstream_failures`, `audit_write_failures`, `stub_decisions`) exposed as JSON at `GET /metrics` (bearer token required when `metrics_token` is set); counters never affect decisions. Every tool-call response carries `X-Do-Harness-Governance: enforced|stub`.

Run:

<!-- example: id=guardian_proxy_commands classification=illustrative reason="Guardian proxy cargo commands" -->
```bash
cargo check -p guardian-proxy
cargo check -p guardian-proxy --features agt-governance
cargo test -p guardian-proxy
cargo test -p guardian-proxy --features mcp-surface
cp crates/guardian-proxy/guardian-proxy.example.toml guardian-proxy.toml
cargo run -p guardian-proxy -- --config guardian-proxy.toml
cargo run -p guardian-proxy -- --verify-audit guardian-audit.jsonl
cargo run -p guardian-proxy -- policy test \
  --policy crates/guardian-proxy/guardian-policy.example.yaml \
  --fixtures crates/guardian-proxy/tests/fixtures/policy
```

## Policy schema (v1)

Policies are YAML files with `version` and `rules`, evaluated in file order:
the first matching rule decides, and a call matching no rule is denied by
default. Every field is validated; unknown fields are rejected.

<!-- example: id=guardian_policy_schema classification=illustrative reason="Policy schema reference; not executed" -->
```yaml
version: 1
rules:
  - action: deny          # allow | deny | ask
    tool: "fs.remove"     # glob pattern; "*" matches anything
    caller: "*"           # glob over caller identity; default "*"
    params:               # optional; every key must match
      path: "/etc/*"      # string expectations are globs
    reason: "never remove system files"
```

- `tool` and `caller` are glob patterns (`*` matches any run of characters).
- `params` values: string expectations are globs against string parameters;
  any other value compares for exact JSON equality. An absent parameter key
  never matches.
- `action: ask` means the call needs human approval: the runtime (which has
  no interactive approval channel) maps it to a deny; `policy test` reports
  it as `ask`.
- A call matching no rule is denied (`no policy rule matched (default deny)`).

### Policy test fixtures

Each file in the fixture directory is JSON or YAML:

<!-- example: id=guardian_policy_fixture classification=illustrative reason="Fixture format reference; not executed" -->
```json
{
  "name": "deny-etc",
  "request": {"tool": "fs.remove", "params": {"path": "/etc/passwd"}, "caller": "ci"},
  "expected": "deny"
}
```

`expected` is `allow`, `deny`, or `ask`; any mismatch or unreadable fixture
makes the command exit non-zero. A request's `caller` is only populated on
the HTTP transport from the `X-Do-Harness-Caller-Id` header (client-supplied
— strip/replace it at an authenticating gateway in front of the proxy).

See `plans/agt-governance-epic.md` for promotion criteria and
`plans/mcp-conformance-epic.md` for the MCP conformance decision and slices.
