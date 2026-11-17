# guardian-proxy: Transport v1 removal and migration

> **Status:** deprecation announced 2026-11-16 — removal target **v1.0.0**
> **Surface:** `guardian-proxy` (`crates/guardian-proxy`)

## What changes

The flat REST tool-call transport (`POST /mcp/tools/call`) predates the MCP
Streamable HTTP surface. Both mediate through the same `ProxyMediator` and the
same fail-closed policy, but keeping two ingress paths keeps two attack
surfaces, two response shapes, and two sets of client bugs alive.

Transport v1 is removed in **v1.0.0**. Until then it stays mounted and every
request announces the removal target:

- response header `deprecation: true`;
- `link: </mcp>; rel="successor-version"` when the binary is built with
  `mcp-surface`;
- one structured log line on the proxy's stderr:
  `{"event":"transport_v1_deprecated","route":"/mcp/tools/call","successor":"/mcp","removal_target":"1.0.0"}`.

The constant is `guardian_proxy::TRANSPORT_V1_REMOVAL_TARGET`, so the header,
the log line, and the docs cannot drift apart.

## Route mapping

| Transport v1 (removed in v1.0.0) | MCP Streamable HTTP |
| --- | --- |
| `POST /mcp/tools/call` with `{"tool": "...", "params": {...}}` | `POST /mcp` with an MCP `tools/call` request |
| governance decision in the response body | governance decision in the MCP response `content` |
| `x-do-harness-governance` response header | unchanged (still emitted) |
| bearer token in `ingress_token` | unchanged |

The body shape is the protocol change: v1 carried a bespoke JSON object, while
`/mcp` speaks JSON-RPC 2.0 and is served only for the modern protocol revision
(`mcp-surface`). A client that already speaks MCP needs no change beyond the
URL.

## Migrating

1. Point the client at `POST /mcp` instead of `POST /mcp/tools/call`.
2. Update the request body to an MCP `tools/call` request.
3. Keep the ingress bearer token and the upstream allowlist exactly as they are;
   mediation, audit, and metrics are shared.
4. Watch the proxy log: any line with `transport_v1_deprecated` is a client you
   have not migrated.

## Removal is a code gate, not a comment

`create_router_with_transport(state, false)` serves 404 for the v1 route while
`/mcp`, `/health`, and `/metrics` stay mounted. `server::tests` pins both
directions: the default router still serves v1, and the removal configuration
returns 404. When v1.0.0 is cut, the default flips to `false` and the removal
test becomes the production behaviour.

## Why not remove it now

The route is the documented ingress for adapters built before `mcp-surface`
existed, and it is exercised by the conformance fixtures. Removing it in a
minor release would break those clients without a migration window; the
deprecation notice, the successor link, and this guide are that window.
