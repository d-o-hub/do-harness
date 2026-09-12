# Epic: MCP Conformance for guardian-proxy

> **Status:** decided via spike (2026-09-12); implementation not started
> **Related:** spec 2026-07-28, `plans/agt-governance-epic.md` criterion (b), issue #40
> **Method:** `spike-and-resolve` → `vertical-event-slice` per slice
> **Owner:** orchestrator swarm

## Why

`guardian-proxy` is the sole tool-call mediation surface, but it exposes a flat
`{"tool","params"}` REST route (`crates/guardian-proxy/src/proxy.rs:9`,
`server.rs:52`). It does not speak Model Context Protocol: no `jsonrpc`/`id`/
`method`, no `MCP-Protocol-Version`/`Mcp-Method`/`Mcp-Name` headers, no
`server/discover`, no `Origin` validation, and no conformance tests. The AGT
promotion criterion (b) ("adds an MCP/tool-call mediation surface",
`plans/agt-governance-epic.md:21`) therefore over-claims. The current MCP spec
is **2026-07-28**: POST-only endpoint, per-request `_meta` metadata instead of
sessions/initialize, required mirrored headers, `server/discover` mandatory,
and `Origin` validation mandatory. This epic makes the proxy genuinely
conformant before AGT promotion can be reviewed.

## Spike findings (2026-09-12, trace 24)

**Hypothesis:** the official Rust SDK (`rmcp`) can front the proxy's axum
ingress without re-architecting mediation, at an acceptable dependency cost.

| Option | Evidence | Verdict |
|--------|----------|---------|
| Adopt `rmcp` 3.3.0 (server ingress) | Apache-2.0, MSRV 1.88. `StreamableHttpService` is a `tower::Service`; mounted and served on the existing **axum 0.7.9** (`nest_service("/mcp", ...)`). `ServerHandler::on_custom_request(CustomRequest) -> CustomResult` plus `RequestContext.id` gives raw method/params passthrough; client peer `send_request(ClientRequest::CustomRequest(...))` does the same upstream. `discover` + `supported_protocol_versions` implemented (`handler/server.rs:393,304`); transport enforces SEP-2243 headers and `Origin`/DNS-rebinding (`transport/common/mcp_headers.rs`, `streamable_http_server/tower.rs:678,866`). Server subtree adds 77 crates (probe total 97 vs current 134). | **Chosen** |
| Hand-rolled JSON-RPC on axum 0.7 | No new deps, keeps MSRV 1.85, full control of fail-closed/audit semantics, ~300–500 LOC. But we own version negotiation, header validation, discover, and future spec churn with no conformance suite. | Rejected |
| Re-scope/rename ("MCP-like REST") | Cheapest and truthful today, but leaves promotion criterion (b) permanently unmet and the strategic gap open. | Rejected |

**Costs accepted, gated behind the feature:** MSRV 1.88 only when
`mcp-surface` is enabled (default builds stay on 1.85); no `client` feature
(avoids a `reqwest 0.13` duplicate against the existing 0.12 under
`deny.toml` `multiple-versions = "deny"`); egress stays on the current
`reqwest` client.

**Decision:** adopt `rmcp` 3.3.0 behind an optional `mcp-surface` feature
(off by default, mirroring `agt-governance`): server-side ingress only, egress
unchanged. Hand-rolled and re-scope rejected.

## Slices (HTN methods)

| Slice | Method | Exit criteria |
|-------|--------|---------------|
| `feat-mcp-ingress-scaffold` | vertical-event-slice | `mcp-surface` feature + optional `rmcp` (server, transport-streamable-http-server, macros) pinned like `agent-governance`; `StreamableHttpService` mounted at `/mcp` with JSON responses and stateless 2026-07-28 mode; legacy routes intact; feature-off builds zero `rmcp`; CI checks/tests both ways |
| `feat-mcp-tools-forwarding` | vertical-event-slice | `ServerHandler`: `get_info` advertises `tools`; `list_tools` forwards `tools/list` upstream (raw JSON-RPC over `reqwest`) and returns typed results; `call_tool` runs `ProxyMediator::decide` → audit → metrics → forward with `params.arguments`; `on_custom_request` passes other methods through; deny/error paths fail closed |
| `feat-mcp-protocol-security` | vertical-event-slice | `supported_protocol_versions = [2026-07-28]`, `discover` served; `allowed_origins` config with localhost default; negative-path tests for protocol-version mismatch, header mismatch (`-32020`), unsupported version (`-32022`), unknown method (404/`-32601`), notification `202` |
| `test-mcp-conformance-suite` | vertical-event-slice | hermetic end-to-end suite over Streamable HTTP (raw `reqwest` requests, **no** `rmcp` client dev-dep) with in-process upstream: allow/deny/audit/metrics on the MCP path, 1 MiB cap, body/header mismatch, degraded mode |
| `docs-mcp-surface` | vertical-event-slice | README/`docs/threat-model-proxy.md`/`docs/compliance.md`/`docs/cli.md` describe the MCP endpoint and feature; flat REST route marked deprecated; AGT epic criterion (b) re-wording; `plans/invariants.json` entry for the optional-feature rule seeded to libSQL |
| `chore-mcp-promotion` | decision | mcp-surface default-on, flat REST route removed, MSRV 1.85→1.88 bump ratified; requires invariants review. No code until this gate |

## Budgets

- **LOC:** ingress ≤150; forwarding/mediation ≤350 (split modules before 450);
  protocol/security ≤150; conformance tests ≤450/file. All within the 500 cap.
- **Dependencies:** one optional crate (`rmcp`, pinned minor with caret range
  documented like `agent-governance`); no new unconditional deps.
- **Feature isolation:** `cargo check -p guardian-proxy` (off) and
  `cargo test -p guardian-proxy --features mcp-surface` (on) are the gates;
  `do-harness.toml` gains an `mcp-surface` check analogous to `check-agt`.

## Non-goals (v1)

- stdio transport (hosts that need it can front the HTTP endpoint).
- Legacy `initialize`/session passthrough: modern `2026-07-28` only; upstream
  must speak modern MCP or sit behind a compatibility shim.
- Multi-upstream aggregation, tool-name namespacing, OAuth, Tasks/Apps
  extensions, SSE streaming (`json_response` mode first; SSE is optional for
  servers).

## Risks and rollback

- `rmcp` 3.x API churn: optional feature + pinned minor; `cargo update`
  reviewed with `cargo deny check`/`cargo audit`.
- Conformance drift with spec revisions: the conformance suite plus rmcp
  release notes review at each bump.
- Rollback: disable `mcp-surface` (default) → behavior identical to today;
  the legacy route stays until `chore-mcp-promotion`.

## Verification checklist

- [ ] `cargo test -p guardian-proxy` (feature off) and `--features mcp-surface` green
- [ ] `cargo clippy --workspace --all-targets -- -D warnings`
- [ ] `do-harness verify` green (including the new feature-on sensor)
- [ ] `cargo deny check` clean with no `reqwest` duplicate
- [ ] conformance suite covers allow/deny/audit/metrics + negative header/version paths
- [ ] AGT epic criterion (b) re-worded; `chore-agt-promotion` still gated on GA
