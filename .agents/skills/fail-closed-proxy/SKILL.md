---
name: fail-closed-proxy
description: >
  Build fail-closed HTTP tool-call mediation (decide, audit, forward) with
  observability counters. Use when adding a proxy endpoint that must deny on
  invalid params, governance denial, or mediator error, forward only on allow,
  and expose audit/metrics evidence. Triggers: "proxy", "fail-closed",
  "tool-call mediation", "guardian", "audit log", "proxy metrics".
license: MIT
metadata:
  version: "0.1.0"
  tags: proxy fail-closed axum audit metrics mediation
---

# Fail-Closed Proxy Skill

## Purpose
Ship an HTTP sidecar that mediates tool calls with deny-by-default semantics
plus audit and metrics evidence. Decisions never depend on observability.

## Method: Decide-Audit-Forward
1. **Decide** via `ProxyMediator::decide(&call)`; on `Err`, convert to
   `Deny { reason: "mediator error: ..." }`. Never allow on error.
2. **Count** the decision (`allow`/`deny`, plus `mediator_errors` on `Err`)
   before any I/O.
3. **Audit** best-effort: append `Allow`/`Deny` to the hash-chained JSONL log;
   on append failure increment `audit_write_failures` and keep the decision.
4. **Forward** only on `Allow` via `POST` to upstream; map outcomes to status:
   `Deny` -> `403`, upstream send/read failure -> `502` +
   `upstream_failures`, body read -> `upstream_ok`.

## Routes
- `GET /health` — liveness probe (always 200).
- `GET /metrics` — JSON snapshot of `ProxyMetrics` (always 200).
- `POST /mcp/tools/call` and `POST /` — mediation entry points.

## Counters (`ProxyMetrics`)
`allow`, `deny`, `mediator_errors`, `upstream_ok`, `upstream_failures`,
`audit_write_failures`. Expose via `GET /metrics`; counters never change
routing.

## State Sharing
- Hold `metrics: Arc<ProxyMetrics>` inside shared `AppState`.
- Build routers with `create_router_with_state(state)` so tests can clone the
  metrics handle before moving state in.
- Keep `AppState` in its own module when `server.rs` nears 450 LOC.

## Tests (at least these)
- `403` on invalid params (non-object `params`).
- `502` on unreachable upstream with `upstream_failures == 1`.
- Allow/deny counting plus `/metrics` snapshot keys.
- Audit chain: allow then deny appends verify via `AuditLog::verify`.

## Gotchas
- Audit failure must not flip the decision — count it, return the original.
- Validate `params`: non-object JSON is `Deny`, not a forwarded call.
- `deny_unknown_fields` on config and call types for forward compatibility.
