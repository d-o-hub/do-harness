# Threat model: guardian-proxy

`crates/guardian-proxy` is an optional, off-by-default HTTP sidecar that
mediates tool calls. This model covers its HTTP surface and audit log.

## Assets

- **Decision integrity** — an allow must never be a fail-open error.
- **Audit chain** — the JSONL log must be tamper-evident.
- **Upstream reachability** — the proxy must not become an SSRF pivot.
- **Local resources** — request/response bodies and timeouts must be bounded.

## Trust boundaries

1. Caller → proxy (`POST /mcp/tools/call`): untrusted JSON body and headers.
2. Proxy → upstream (`ProxyConfig.upstream`): operator-configured, must pass
   the SSRF policy at construction.
3. Proxy → audit file: local filesystem, possibly multiple processes.
4. Operator → `/metrics`: observability data may reveal tool names.

## Threats and mitigations

| Threat | Mitigation | Evidence |
|---|---|---|
| SSRF to cloud metadata | Link-local/metadata hosts are always rejected; loopback/private need `allow_private_upstreams`; optional exact-host allowlist | `validate_upstream` in `crates/guardian-proxy/src/proxy.rs` |
| Fail-open on mediation error | Only an explicit `Allow` is forwarded; policy/runtime errors map to 403 | `tool_call_handler`, `ProxyMediator::decide` |
| Governance unavailability hidden | Mediator init failure serves a degraded router: `/health` 503, all calls denied | `AppState::degraded`, `create_router_degraded` |
| Stub mistaken for enforcement | Startup banner, `X-Do-Harness-Governance: enforced\|stub` header, `stub_decisions` counter | `main.rs`, `server.rs`, `metrics.rs` |
| Oversized request/response DoS | 1 MiB ingress limit (`DefaultBodyLimit`) and bounded upstream reads | `server.rs` |
| Hung upstream pins a task | 30s `reqwest` client timeout | `state.rs` |
| Audit chain forked across processes | Exclusive advisory file lock + re-read of the tail + `fsync` per append | `audit_log.rs` (`fs2` lock, `tail_of`) |
| Audit append failure silent | Best-effort append never changes the decision, but failures raise a stderr alert and increment `audit_write_failures` | `record_audit` |
| Metrics disclosure | Optional bearer token via `metrics_token`; bind loopback in the example config | `metrics_handler`, example config |

## Residual risks

- Inbound HTTP is plain by default: deploy behind a TLS terminator or bind to
  loopback when remote clients are untrusted.
- The SSRF check is syntactic host matching, not DNS resolution; a hostname
  that resolves to a private address is not detected. `upstream_allowlist`
  mitigates by pinning known hosts.
- The advisory file lock protects cooperation between proxy instances; a local
  attacker with write access to the audit file is outside this model (the hash
  chain detects tampering on the next read).
- The `AgentMeshClient` (pre-GA AGT) exposes a boolean allow today; the typed
  error channel exists so a future error surface can be denied explicitly.
