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

## Migrating from code-embedded policy to declarative policy

Before the declarative policy layer, governance lived in Rust: the default
build permitted well-formed calls (stub), and the `agt-governance` build
delegated to the external AGT agent mesh. Policy changes required a Rust
change, a rebuild, and a redeploy. A versioned YAML policy file
(`ProxyConfig.policy_file`) externalizes the same decision surface for review,
test, and shipping alongside application code.

### What changes

- **Startup:** when `policy_file` is set, `ProxyMediator` loads and validates
  the policy before serving; a missing or unparseable file fails mediator
  construction and the proxy serves degraded (`/health` 503, all calls
  denied). With no `policy_file`, the previous code-embedded defaults apply
  unchanged.
- **Decisions:** a loaded policy is evaluated first. Only an explicit policy
  `allow` reaches the governance gate (AGT when `agt-governance` is built,
  the well-formed-params check otherwise); `deny` stops, and `ask` maps to a
  deny (no interactive approval channel exists in the proxy). A call matching
  no rule is denied.
- **Evidence:** every audit record carries the policy file's SHA-256 digest
  (`policy_sha256`), bound into the chain hash, so a ledger proves which
  policy produced each decision.

### Migrating

1. Author a policy file from the schema reference in the crate README; start
   from `crates/guardian-proxy/guardian-policy.example.yaml`.
2. Set `policy_file` in the proxy config to the policy path.
3. Test the policy offline before deploying:
   `guardian-proxy policy test --policy <file> --fixtures <dir>` — fixture
   cases cover allow/deny/ask and exit non-zero on any mismatch.
4. Deploy the policy file alongside the service and ship changes to it like
   any other reviewed artifact; the audit ledger records its digest per
   decision.

Caller-based rules (`caller` patterns) rely on the
`X-Do-Harness-Caller-Id` request header on the deprecated flat transport,
which is client-supplied: run the proxy behind an authenticating gateway that
strips untrusted caller headers. The MCP surface does not currently set a
caller identity, so caller-specific rules treat MCP calls as unnamed.
