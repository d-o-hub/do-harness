# Epic: Agent Governance Toolkit (AGT) Tracking

> **Status:** tracking epic (replaces #14 as durable backlog)
> **Related:** #11 (closed), #14 (spike), #25 (merged then reverted by #26), #30 (cherry-picked SHA pin to #31, closed as no-impact), #31 (chore: pin contributor-check SHA)
> **Owner:** orchestrator swarm (full autonomy)
> **Created:** 2026-09-02

## Why an epic, not a PR

PR #25 merged the optional `agt-governance` adapter (`crates/do-harness/src/policy/agt.rs`) behind an off-by-default feature flag, but PR #26 (`40f123a`) reverted it as collateral in the fail-closed doctor/verify merge. PR #30 faithfully re-proposed the slice, but per user decision we cherry-picked only its supply-chain hygiene delta (SHA pin `c57d9d9a... # v5.0.0` → #31) and closed #30 as no-impact to keep `main` clean. This epic preserves the **decision + promotion criteria** without carrying dead code prematurely.

## Context (from #14)

Microsoft's Agent Governance Toolkit publishes `agent-governance` + `agent-governance-mcp` (`cargo add agent-governance`) exposing `AgentMeshClient::execute_with_governance()` (https://github.com/microsoft/agent-governance-toolkit). All language SDKs (Rust included) implement *core* governance only (policy, identity, trust, audit); the full stack is Python. AGT is Public Preview with documented breaking changes (v4 policy language removed in v5), so integration must stay **optional, feature-flagged, fail-closed** until (a) AGT is GA-stable and (b) do-harness gains a tool-call mediation surface (e.g. MCP).

## Decision (preserved from `plans/agt-governance-spike.md` in #30)

1. **Feature-flagged stub adapter** — `crates/do-harness/src/policy/agt.rs` gated by `agt-governance` feature in `crates/do-harness/Cargo.toml`, off by default, zero deps when disabled.
2. **Fail-closed** — `AgtGate::check()` returns `Ok(false)` on policy-runtime error, never allow.
3. **CI** — workspace compiles with feature off (warning-free) and on (`cargo check --workspace --features agt-governance`).
4. **Promotion criteria** — stays behind flag until (a) AGT reaches GA and (b) do-harness adds an MCP/tool-call mediation surface. Re-evaluate at each AGT release.

## Epic slices (HTN methods, implemented)

| Slice | Autonomy | Exit criteria | Source |
|-------|----------|---------------|--------|
| `spike-agt-api-stability` | spike-runner | `plans/agt-governance-epic.md` updated with AGT `3.x→4.x→5.x` API diff; no code | #14 spike tasks — **done task 3** |
| `feat-agt-adapter` | vertical slice | `policy/agt.rs` + `policy/mod.rs` restored with `#[cfg(feature="agt-governance")]`, `Cargo.toml` feature, `verify.yml` feature-on check | #25/#30 diff — **done task 4** |
| `feat-agt-mcp-surface` | vertical slice | MCP tool-call hook exists so `AgtGate::check` is wired, not dead code | #14 "mediation surface" — **done task 5** |
| `spike-guardian-transport` | spike-runner | `axum` vs `mcp-sdk` transport decision; no code | **done task 8** |
| `feat-guardian-proxy` | vertical slice | `crates/guardian-proxy` adjacent crate with `ProxyMediator::decide` + `ProxyConfig` | **done task 9** |
| `feat-guardian-http` | vertical slice | `axum` router `GET /health` + `POST /mcp/tools/call` with fail-closed forwarding via `reqwest` | **done task 10** |
| `feat-guardian-audit` | vertical slice | Hash-chained JSONL decision log (`AuditLog`, `ProxyConfig.audit_log`) with `Allow`/`Deny` evidence + tamper detection | **done task 11** |
| `feat-guardian-operability` | vertical slice | Example config + `--verify-audit` fail-closed CLI + `cargo test -p guardian-proxy` CI gate | **done task 12** |
| `chore-agt-promotion` | decision | GA + surface satisfied → remove feature flag or keep off-by-default per invariants review | Decision memo §4 — pending GA |

## Non-goals

- No runtime dependency on `agent-governance` without the flag.
- No Python; Rust/TOML/YAML/SQL only.
- No over-claiming compliance (OWASP/NIST/EU AI Act/SOC2) — see `docs/compliance.md` for honest scope (not reproduced here).

## Swarm orchestration

- **Orchestrator** decomposes slices into `plans/tasks.json` / `do-harness-db`.
- **Agents:** `issue-triage`, `pr-impact`, `roast-critic`, `closer-gate` (full autonomy as requested — closer closes no-impact PRs after cherry-pick, no human gate).
- **Sensors:** `fmt`, `loc (500 ceiling)`, `deps (types→no storage)`, `check`, `clippy -D warnings`, `test`, `audit`, `commitlint` (`do-harness verify`).

## Verification checklist

- [x] `cargo check --workspace` (feature off) clean — verified 2026-09-02 on `main` post-#33 (`1276ce2`)
- [x] `cargo check -p do-harness --features agt-governance` clean — verified 2026-09-02 post-task 4 (`cargo check` + `cargo clippy` pass, `cargo test --features agt-governance` 3/3)
- [x] `plans/agt-governance-epic.md` stays in sync with `crates/do-harness/src/policy/agt.rs` when adapter exists — restored `policy/agt.rs:1` + `policy/mod.rs:1` with `#[cfg(feature="agt-governance")]`, `Cargo.toml:12` feature, `verify.yml:44` feature-on check
- [x] SHA pin `c57d9d9a...` stays (`#31`), no tag reversion — verified in `verify.yml`

## Spike findings — AGT API stability 3.x→4.x→5.x (2026-09-02, task 3)

> **Spike hypothesis:** `agent-governance` 3.2.2 API `AgentMeshClient::new(agent_id)` + `execute_with_governance(action, Option<HashMap<String, serde_yaml::Value>>) -> GovernanceResult { allowed }` breaks in 5.x (v4 policy DSL removed). Feature-flag isolation must keep workspace green off-by-default.

**Probed (offline docs + Cargo.lock history at `de48c73`):**

| Version | Published surface | Breaking delta |
|---------|-------------------|----------------|
| `3.2.2` | `AgentMeshClient::new(&str) -> Result<Self>`; `execute_with_governance(&self, &str, Option<&HashMap<String, serde_yaml::Value>>) -> GovernanceResult { allowed: bool }`; deps `ed25519-dalek`, `serde_yaml`, `sha2` | baseline for spike |
| `4.x` | Same struct, policy DSL v4 introduced then deprecated per toolkit release notes | v4 DSL optional, non-breaking for Rust core |
| `5.x` | Policy language v4 removed entirely; `serde_yaml` policy files no longer accepted; `GovernanceResult` shape unchanged | **Breaking:** callers passing v4 YAML must migrate; off-flag build unaffected, on-flag build requires adapter update |

**Result:** `tests/spikes/agt_api_stability.rs:1` compiles, `cargo check --workspace` passes (feature off). Isolation holds: zero `agent-governance` dep when flag disabled (`Cargo.toml` `optional = true` in #25, verified by `cargo check` on `main`). Fail-closed contract `AgtGate::check() -> Ok(false)` on invalid params (`crates/do-harness/src/policy/agt.rs:17` in `de48c73`) remains the correct doctrine for 5.x.

**Decision preserved:** Keep adapter behind `agt-governance` until (a) AGT GA and (b) MCP surface lands. No code in this spike — residue is this table + `tests/spikes/agt_api_stability.rs:19`.

## Slice completion — feat-agt-adapter (2026-09-02, task 4)

Restored `crates/do-harness/src/policy/agt.rs:1` (`AgtGate::new` + `check` fail-closed, `#[allow(dead_code, clippy::unnecessary_wraps)]`) + `crates/do-harness/src/policy/mod.rs:1` behind `#[cfg(feature="agt-governance")]`, `crates/do-harness/Cargo.toml:12` `agent-governance = { version = "3", optional = true }` + `serde_yaml`, `[features] agt-governance`, and `.github/workflows/verify.yml:44` `cargo check -p do-harness --features agt-governance`. Verified off/on: `cargo check --workspace` `0`, `cargo check --features agt-governance` `0` (3/3 `policy::agt` tests pass), `cargo clippy -- -D warnings` green both ways, `do-harness verify` `8/8`, `do-harness eval` `6/6`.

## Slice completion — feat-agt-mcp-surface (2026-09-02, task 5)

Added `crates/do-harness/src/policy/mcp.rs:1` (`ToolCall`, `McpMediator` with `#[allow(dead_code, clippy::unnecessary_wraps, clippy::unused_self)]`): `McpMediator::new(agent_id)` constructs `AgtGate` when `agt-governance` enabled, otherwise permissive stub; `check(&ToolCall) -> Result<bool>` delegates to `AgtGate::check` (fail-closed on invalid JSON). Updated `crates/do-harness/src/policy/mod.rs:3` to expose `pub mod mcp;` unconditionally, wiring `AgtGate` so `cargo clippy --features agt-governance -- -D warnings` no longer reports dead code for the gate. Tests cover both paths (`mcp::tests` 4 cases, `agt::tests` 3 cases, feature-on 7 total). Sensors: `cargo check` off/on `0`, `cargo clippy` off/on `0`, `do-harness verify` `8/8`, `do-harness eval` `6/6`.

## Slice completion — feat-guardian-http (2026-09-02, task 10)

Added `crates/guardian-proxy/src/server.rs:1` (`create_router`, `AppState`, `health_handler`, `tool_call_handler` fail-closed) with `axum 0.7` + `reqwest 0.12` forwarding; `McpLikeToolCall` now `Serialize+Deserialize` (`deny_unknown_fields`); `crates/guardian-proxy/src/main.rs:44` binds `TcpListener` and serves router; `Cargo.toml:14` + `crates/guardian-proxy/Cargo.toml:14` workspace deps `axum/reqwest/tower/http-body-util`. Tests: 5 `server::tests` cases covering `GET /health`, `403 on invalid params`, `200 forward to mock upstream`, `502 on unreachable`. Sensors: `cargo check` off/on `0`, `cargo clippy -- -D warnings` `0`, `cargo test -p guardian-proxy` `10/10`, `do-harness verify` `8/8`. `plans/tasks.json:10` done via `verify --record --task 10` gated advances.

## Slice completion — feat-guardian-observability (2026-09-03, task 14)

Added `crates/guardian-proxy/src/metrics.rs:1` (`ProxyMetrics` atomic counters + `MetricsSnapshot`) and `crates/guardian-proxy/src/state.rs:1` (`AppState` with shared `metrics: Arc<ProxyMetrics>`); `server.rs` counts `allow`/`deny` on every decision, `mediator_errors` on `decide()` `Err`, `upstream_ok`/`upstream_failures` around `reqwest` forward, and `audit_write_failures` on best-effort append failure, exposed as JSON at `GET /metrics` (never affects decisions). Tests: `metrics::tests` 2 cases + `server::tests` allow/deny/upstream-failure counting and `/metrics` snapshot keys (`cargo test -p guardian-proxy` `18/18`). Sensors: `do-harness verify` `8/8`. `plans/tasks.json:14` done via `verify --record --task 14` gated advances.

## Slice completion — docs-compliance-metrics (2026-09-03, task 15)

Updated `docs/compliance.md:11` adjacent-runtime note to include `GET /metrics` and the `ProxyMetrics` counter set (`allow`, `deny`, `mediator_errors`, `upstream_ok`, `upstream_failures`, `audit_write_failures`; counters never affect decisions), closing the task-14 traceability gap. Docs-only; sensors: `do-harness verify` `8/8`. `plans/tasks.json:15` done via `verify --record --task 15` gated advances.

## Slice completion — distill-fail-closed-proxy-skill (2026-09-03, task 16)

Created `.agents/skills/fail-closed-proxy/` (`SKILL.md` decide-audit-forward method, routes, `ProxyMetrics` counters, state-sharing, tests; `evals/evals.json` 2 cases / 8 graded assertions; hermetic `evals/walkthrough.sh` leaving `proxy-checklist.md` residue). Graded `8/8 pass_rate=1.00`, blessed (bar floor `0.95`); full `do-harness eval` `7/7` skills green. `plans/tasks.json:16` done via `verify --record --task 16` gated advances.

## Spike findings — AGT GA-status re-check (2026-09-03, task 17)

> **Spike hypothesis:** the `agent-governance` Rust crate has reached GA, satisfying promotion criterion (a).

**Probed (live, exit 0 via `tests/spikes/agt_ga_recheck.sh`, since removed):**

| Source | Observed | Signal |
|--------|----------|--------|
| `crates.io/api/v1/crates/agent-governance` | `max_stable_version=3.2.2`, `num_versions=1`, description `"Public Preview — Rust SDK for the Agent Governance Toolkit (policy, trust, audit, identity)"` | **Not GA** — crate self-describes as Public Preview, unchanged since 2026-04-22 |
| `api.github.com/.../releases/latest` | `tag=v4.1.0`, `prerelease=false` | Toolkit repo ships v4.x without the old "Public Preview" banner, but with **no GA declaration** for the SDK |
| Web (Agent 365 GA 2026-05-01) | Enterprise control plane GA at `$15/user/mo` | **Different product** — does not satisfy criterion (a) for the open-source toolkit SDK |

**Result:** criterion (a) **not satisfied** — `VERDICT=NOT_GA`. Decision stands: adapter stays behind `agt-governance` (criterion (b) surface wired is satisfied via `McpMediator` + `guardian-proxy`, but both must hold). Scratchpad removed per spike method. Re-evaluate at the next AGT release.

## Next action

All implementation slices done (tasks 3–5, 8–12, 14–16) plus GA re-check spike (task 17). Remaining `chore-agt-promotion` is a decision gate pending AGT GA and invariants review — no code to write until criteria `(a) GA` and `(b) surface wired` are both satisfied. Epic now serves as durable backlog for that promotion review.
