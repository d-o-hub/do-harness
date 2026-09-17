# Epic: MCP Conformance for guardian-proxy

> **Status:** decided via spike (2026-09-12); slices 1–5 complete; `chore-mcp-promotion` decided 2026-09-13 (keep off-by-default)
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
- `x-mcp-header` parameter mirroring: only the standard `Mcp-Method`/`Mcp-Name`
  headers are forwarded; tools requiring `Mcp-Param-*` fail closed upstream.

## Risks and rollback

- `rmcp` 3.x API churn: optional feature + pinned minor; `cargo update`
  reviewed with `cargo deny check`/`cargo audit`.
- Conformance drift with spec revisions: the conformance suite plus rmcp
  release notes review at each bump.
- Rollback: disable `mcp-surface` (default) → behavior identical to today;
  the legacy route stays until `chore-mcp-promotion`.

## Verification checklist

Re-verified 2026-09-17 against the shipped tree. The slices had landed but the
checklist was never ticked, so it read as open work while every item passed:

- [x] `cargo test -p guardian-proxy` (feature off) and `--features mcp-surface` green — 36 and 57 passed, 0 failed
- [x] `cargo clippy --workspace --all-targets -- -D warnings` — exit 0
- [x] `do-harness verify` green (including the new feature-on sensor) — 13/13 sensors, `check-mcp` among them
- [x] `cargo deny check` clean with no `reqwest` duplicate — advisories/bans/licenses/sources ok, and the `mcp-surface` closure resolves a single `reqwest v0.12.28`
- [x] conformance suite covers allow/deny/audit/metrics + negative header/version paths — `call_tool_forwards_after_allow`, `denied_call_maps_to_tool_error_with_audit`, `audit_records_allowed_mcp_call`, `tool_call_round_trip_records_audit_and_metrics`; negatives in `src/mcp/tests/security.rs` (missing/unsupported/mismatched protocol version, tool-name mismatch, Origin denial)
- [x] AGT epic criterion (b) re-worded; `chore-agt-promotion` still gated on GA — `plans/agt-governance-epic.md` criterion (b) wording plus the GA watch `VERDICT=NOT_GA`

## Slice completion — feat-mcp-ingress-scaffold (2026-09-12)

- `Cargo.toml`: `rmcp = "=3.3.0"` (features `server`,
  `transport-streamable-http-server`, `macros`) with the same exact-pin
  exemption comment as `agent-governance`; `crates/guardian-proxy/Cargo.toml`
  adds an optional dep plus `mcp-surface` feature (off by default, so default
  builds keep MSRV 1.85).
- `crates/guardian-proxy/src/mcp.rs`: `McpIngress` handler
  (`ServerInfo.server_info = guardian-proxy <crate version>`) advertising only
  `2026-07-28`; `service()` configures stateless modern mode, JSON responses,
  required per-request protocol metadata, and the 1 MiB body cap; `mount()`
  nests the tower service at `/mcp`.
- `server.rs`: `create_router_with_state` mounts the endpoint when the feature
  is on; the explicit legacy `/mcp/tools/call` route keeps precedence (proved
  by the feature-on legacy suite).
- Tests: `mcp::tests` 3 cases (discover returns `supportedVersions`, missing
  `MCP-Protocol-Version` is 400, legacy route still 503 degraded);
  `test_mcp_endpoint_absent_without_feature` asserts 404 with the feature off.
  Feature-off 29 tests / feature-on 31.
- CI/sensors: new `check-mcp` sensor (`cargo test -p guardian-proxy --features
  mcp-surface`) added to the verification and release sets (`do-harness.toml`);
  `verify.yml` test step now covers the feature.
- Evidence: `cargo clippy -p guardian-proxy --all-targets -- -D warnings`
  off/on green; `cargo check -p guardian-proxy --features
  agt-governance,mcp-surface` green; `cargo deny check` clean (no duplicate
  versions); `do-harness verify` 12/12; trace 25 + harness heuristic 18.
- Note: rmcp's MSRV 1.88 applies only when the feature is enabled; the CI MSRV
  job (1.85, default features) stayed green on the first post-merge run
  (`verify` run 34758546116, 2026-09-13: `msrv` success).
  Next slice: `feat-mcp-tools-forwarding`.

## Slice completion — feat-mcp-tools-forwarding (2026-09-12)

- `crates/guardian-proxy/src/mcp/forward.rs`: raw JSON-RPC forwarding with
  `Mcp-Method`/`Mcp-Name` headers, 1 MiB request/response caps, upstream
  JSON-RPC error mapping, and an SSE `data:` scan for conformant servers that
  answer `text/event-stream`; all failure classes are protocol errors.
- `mcp.rs` handler: `get_info` advertises the `tools` capability;
  `list_tools` and `on_custom_request` pass through after a readiness check;
  `call_tool` runs `decide` → metrics (allow/deny/stub/mediator-error) →
  `record_audit`, forwards only on `Allow` with `params.arguments` intact, maps
  `Deny` to a caller-visible tool-level error result, and maps degraded/
  mediator/upstream faults to protocol errors. Denied and degraded calls never
  reach upstream (asserted).
- `server.rs`: `record_audit` and `MAX_UPSTREAM_BYTES` widened to `pub(crate)`
  for reuse; legacy behavior unchanged.
- Tests: `mcp::tests` now 9 cases (discover, missing protocol header, legacy
  precedence, `tools/list` forwarding, mediated `tools/call` with `Mcp-Name`
  assertion + metrics snapshot, degraded zero-upstream-calls, custom-method
  passthrough, audit chain intact with `tool`/`decision` fields, unreachable
  upstream). Feature-off 29 / feature-on 37.
- Evidence: clippy `-D warnings` off/on, `agt-governance,mcp-surface` combined
  check, `cargo deny check` clean, `do-harness verify` 12/12; trace 26 +
  `fail-closed-proxy` heuristic 19. Next slice: `feat-mcp-protocol-security`.

## Slice completion — feat-mcp-protocol-security (2026-09-12)

- `ProxyConfig` gains `allowed_origins` (RFC 6454 exact match) with loopback
  defaults (`http://localhost`, `https://localhost`, `http://127.0.0.1`,
  `https://127.0.0.1`); an empty list disables Origin validation, and requests
  without `Origin` always pass. `ProxyMediator::allowed_origins()` feeds
  `AppState` and `StreamableHttpServerConfig::with_allowed_origins`, so the
  spec MUST is enforced instead of rmcp's ignore-Origin default.
- Negative-path coverage added in `mcp::tests` (now 15 cases): allowed origin
  200 vs disallowed 403; unsupported protocol version → 400 `-32022` with the
  supported list; header/body version mismatch → 400 (`-32020` when a JSON-RPC
  body is present); `Mcp-Name` mismatch → 400 `-32020` with zero upstream
  calls; unknown method `x-vendor/missing` forwarded, upstream `-32601`
  propagated as HTTP 404; notification accepted with 202.
- Example config documents the new key
  (`crates/guardian-proxy/guardian-proxy.example.toml`).
- Evidence: feature-off 29 / feature-on 43 tests, clippy `-D warnings` off/on,
  `agt-governance,mcp-surface` combined check, `cargo deny check` clean,
  `do-harness verify` 12/12; trace 27 + `fail-closed-proxy` heuristic 20.
  Next slice: `test-mcp-conformance-suite`.

## Slice completion — test-mcp-conformance-suite (2026-09-12)

- New integration suite `crates/guardian-proxy/tests/mcp_conformance.rs`
  (feature-gated) drives the proxy through real TCP listeners with a raw
  `reqwest` client and an in-process upstream: round trip with audit-chain and
  metrics assertions, 1 MiB body → 413 with zero upstream calls, and degraded
  mode returning a JSON-RPC error without forwarding.
- Deny coverage gap closed: `McpIngress::mediate` now returns the mapped
  denial response, so a direct unit test exercises deny → tool-level error +
  deny metric + deny audit record (the typed `arguments` shape cannot express
  the stub's denied input).
- `mcp/tests.rs` split at the LOC ceiling: negative-path tests moved to
  `mcp/tests/security.rs` (354 + 187 lines).
- `init.rs` decomposed while the loc sensor warned at 452 lines: portable
  skill scaffolding moved to `crates/do-harness/src/init/skills.rs`.
- Evidence: feature-off 29 / feature-on 44 lib + 3 integration tests, clippy
  `-D warnings` off/on, `cargo deny check` clean, `do-harness verify` 12/12;
  traces 28 + harness heuristic 21 + `fail-closed-proxy` heuristic 22.
  Next slice: `docs-mcp-surface`.

## Slice completion — docs-mcp-surface (2026-09-12)

- `crates/guardian-proxy/README.md` documents the `mcp-surface` endpoint,
  header/Origin requirements, caps, and the legacy route's deprecation; the
  root README's CI and runtime-proxy notes mention both optional features.
- `docs/threat-model-proxy.md` adds MCP trust boundaries, five threat rows
  (DNS rebinding/Origin, header/body divergence, version confusion, policy
  ordering, oversized payloads), and residual risks (unauthenticated endpoint,
  empty `allowed_origins`, `Mcp-Param-*` gap, SSE scanned not streamed).
- `docs/compliance.md` corrects the "requires `agt-governance`" wording and
  notes the optional MCP ingress decision path.
- Flat route deprecation is executable: every legacy tool-call response carries
  `Deprecation: true`, plus `Link: </mcp>; rel="successor-version"` when
  `mcp-surface` is enabled (covered by a feature-conditional test).
- `plans/invariants.json` gains the optional `mcp-surface` architecture
  decision; seeded to libSQL with `do-harness seed`.
- `plans/agt-governance-epic.md` re-words criterion (b): satisfied behind the
  feature, with default-on as the `chore-mcp-promotion` decision.
- Evidence: feature-off 30 / feature-on 45 lib + 3 integration tests, clippy
  `-D warnings` off/on, `do-harness verify` 12/12, `do-harness seed` upsert.
  Next slice: `chore-mcp-promotion` (decision gate).

## Promotion review — chore-mcp-promotion (2026-09-13)

**Decision: keep `mcp-surface` off by default; keep the flat route deprecated
but present; do not bump MSRV. Revisit when a default-MCP consumer appears or
at AGT GA promotion.**

| Factor | Finding |
|---|---|
| Surface completeness | slices 1–5 green: 30 tests off / 45 lib + 3 integration on, `cargo deny check` clean, `verify` 12/12 |
| MSRV | every `rmcp` 3.x release (3.0–3.3) requires Rust 1.88; the workspace declares 1.85 (README, CI MSRV job), which holds only while the feature is off |
| Dependency weight | `rmcp` adds ~77 crates when enabled; the feature-on check/test sensors keep it honest |
| Route removal is coupled to default-on | with `mcp-surface` off, `/mcp/tools/call` is the only tool-call ingress; removing it now would leave default builds with no mediation endpoint |
| Deprecation age | `Deprecation: true` + successor `Link` shipped 2026-09-12; removal before a release boundary would make the signal meaningless |
| Consumers | no consumers outside this workspace; `guardian-proxy` is not published |
| AGT | GA watch run 2026-09-13: `VERDICT=NOT_GA`; `chore-agt-promotion` stays blocked |

Consequences: criterion (b) stays satisfied behind the feature (parity with
`agt-governance`), the MSRV promise is unchanged, and no breaking removal
happens without a default-enabled successor. Revisit triggers: (a) a consumer
needs MCP by default, (b) AGT reaches GA and promotion review runs, (c) a
release boundary after which the flat route can be removed together with
default-on.
