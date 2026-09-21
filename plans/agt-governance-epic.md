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
| `chore-agt-promotion` | decision | GA + surface satisfied → remove feature flag or keep off-by-default per invariants review | Decision memo §4 — **hold** (2026-09-15, re-confirmed 2026-09-16: all engineering gates pass, criterion (a) still `VERDICT=NOT_GA`; see Promotion review below) |
| `chore-remove-doharness-policy` | refactor | Delete duplicated `do-harness/src/policy/` (`AgtGate`/`McpMediator`) now that `guardian-proxy` is the sole tool-call mediation surface; drop the `agt-governance` feature from the CLI crate | #34 item 10 — **done task 30** |

## Non-goals

- No runtime dependency on `agent-governance` without the flag.
- No Python; Rust/TOML/YAML/SQL only.
- No over-claiming compliance (OWASP/NIST/EU AI Act/SOC2) — see `docs/compliance.md` for honest scope (not reproduced here).

## Swarm orchestration

- **Orchestrator** decomposes slices into `plans/tasks.json` / `do-harness-db`.
- **Agents:** `issue-triage`, `pr-impact`, `roast-critic`, `closer-gate` (full autonomy as requested — closer closes no-impact PRs after cherry-pick, no human gate).
- **Sensors:** `fmt`, `loc (500 ceiling)`, `deps (types→no storage)`, `check`, `clippy -D warnings`, `test`, `audit`, `commitlint` (`do-harness verify`).

## Verification checklist

- [x] `cargo check --workspace` (feature off) clean — verified 2026-09-02 on `main` post-#33 (`1276ce2`), still run by the CI `msrv` job (`cargo check --workspace --locked`)
- [x] `cargo check -p guardian-proxy` + `cargo check -p guardian-proxy --features agt-governance` + `cargo test -p guardian-proxy` clean — run by `.github/workflows/verify.yml` after the CLI feature was removed (task 30)
- [x] `plans/agt-governance-epic.md` stays in sync with `crates/guardian-proxy/src/proxy.rs` (`ProxyMediator`/`AgtGateWrapper`) now that `crates/do-harness/src/policy/` is deleted (task 30)
- [x] SHA pin `c57d9d9a...` stays (`#31`), no tag reversion — verified in `contributor-check.yml`; `agent-governance` stays pinned `=3.2.2` in the workspace manifest until GA

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

**Result:** criterion (a) **not satisfied** — `VERDICT=NOT_GA`. Decision stands: adapter stays behind `agt-governance`. Criterion (b) is now satisfied by the optional `mcp-surface` ingress: `guardian-proxy` speaks MCP `2026-07-28` (discover, tools/list passthrough, mediated tools/call, protocol/Origin/header validation) behind the off-by-default feature, per slices 1–4 of `plans/mcp-conformance-epic.md`; default-on remains the `chore-mcp-promotion` decision. Both criteria must hold. Scratchpad removed per spike method. Re-evaluate at the next AGT release.

## GA watch automation (tasks 19–21)

Manual re-spikes (task 17) are replaced by a scheduled informational watch:

- `scripts/check-agt-ga.sh` — prints crate/release evidence plus `VERDICT=GA|NOT_GA|UNKNOWN`. `GA` requires **both** the watched `crates.io` description to drop `preview` **and** the latest toolkit release notes to declare GA (non-prerelease); any fetch/parse failure yields `UNKNOWN`, never `GA`. Accepts `--crate-json/--release-json` fixtures for hermetic testing. Not a `verify` sensor by design (network-dependent).
- **Crate rename (2026-09-19):** upstream renamed the Rust SDK `agent-governance` → `agentmesh`. The retired name is frozen at `3.2.2` (2026-04-22) and receives no further publishes; `agentmesh` 4.0.0 is the live crate and upstream's README installs with `cargo add agentmesh`. The watch previously queried only the retired name, so it could never report `GA`: the promotion gate was gated on an instrument that could not fire, and its `NOT_GA` was indistinguishable from a genuine hold. The watch now queries `agentmesh` as the gate, queries the retired name as printed evidence only, prints every queried source URL, and is pinned by `crates/do-harness/tests/agt_ga_watch.rs` (7 hermetic cases). **The workspace pin deliberately stays on the retired `3.2.x` line**: moving to `agentmesh` 4.x is a breaking API change (`serde_yaml::Value` → `agentmesh::policy_data::Value`), so it belongs to the `chore-agt-promotion` decision rather than a routine bump.
- Spike finding (task 19): `crates.io` returns `403` without a `User-Agent` header — the script sends one.
- `.github/workflows/agt-ga-watch.yml` — monthly `cron` + `workflow_dispatch`; `contents: read, issues: write`. On `GA` it opens (or reuses) an `[agt-ga-watch]` tracking issue for human promotion review; `NOT_GA`/`UNKNOWN` only log and never fail the run. The watch never removes the `agt-governance` feature flag itself.

### GA watch run — 2026-09-19 (crate-name correction)

`bash scripts/check-agt-ga.sh` after correcting the queried crate name:

- `source: crate=https://crates.io/api/v1/crates/agentmesh`
- `source: legacy=https://crates.io/api/v1/crates/agent-governance`
- crate: `name=agentmesh version=4.0.0 description=Public Preview — Rust SDK for the Agent Governance Toolkit (policy, trust, audit, identity)`
- legacy: `name=agent-governance version=3.2.2` (retired name, evidence only)
- release: `tag=v4.1.0 prerelease=false`
- `VERDICT=NOT_GA` — criterion (a) remains unsatisfied, now for a live reason: upstream's `CHANGELOG.md` states "All releases are currently **public preview releases**", and the live crate still self-describes as `Public Preview`.

The prior three runs (2026-09-12/15/16) reached the same verdict through a
retired crate name, so they agreed with the truth by coincidence rather than by
measurement. The verdict is unchanged; the instrument now actually measures it.

### GA watch run — 2026-09-21

Manual `bash scripts/check-agt-ga.sh` (exit 0):

- `source: crate=https://crates.io/api/v1/crates/agentmesh`
- `source: legacy=https://crates.io/api/v1/crates/agent-governance`
- `source: release=https://api.github.com/repos/microsoft/agent-governance-toolkit/releases/latest`
- crate: `name=agentmesh version=4.0.0 description=Public Preview — Rust SDK for the Agent Governance Toolkit (policy, trust, audit, identity)`
- legacy: `name=agent-governance version=3.2.2` (retired name, evidence only)
- release: `tag=v4.1.0 prerelease=false`
- `VERDICT=NOT_GA` — a measured verdict, not `UNKNOWN`: both sources fetched and parsed, so criterion (a) is genuinely unsatisfied for the live crate.

The held state was re-checked alongside the verdict, since a hold is only sound
if the artifacts it holds are still intact:

| Check | Result |
|---|---|
| `cargo test -p do-harness --test agt_ga_watch` | 8 passed, 0 failed — the gate keys on the live crate and ignores the retired name |
| `cargo check -p guardian-proxy --no-default-features` | exit 0 |
| `cargo check -p guardian-proxy --features agt-governance` | exit 0 |
| `cargo test -p guardian-proxy` | 36 passed, 0 failed |
| `cargo test -p guardian-proxy --features agt-governance` | 37 passed, 0 failed |
| `target/release/do-harness verify --set verification --record` | 14 passed, 0 failed, 0 skipped — evidence recorded at `859b0f4` |
| `target/release/do-harness status --set verification` | `green` — signal-set evidence current at `859b0f4` |

`crates/guardian-proxy/Cargo.toml` still declares `default = []`. No promotion
review was opened and no artifact changed; the monthly watch keeps collecting
evidence until it reports `GA`. The next scheduled collection is 2026-10-01
09:00 UTC (`cron: 0 9 1 * *`); the workflow was deliberately not re-dispatched by
hand, since a manual run now would only re-measure the same `NOT_GA` the schedule
will measure for itself.

Since the instrument correction (`c6b6484 fix(agt): watch the live agentmesh crate
in the ga gate`) no commit has touched `crates/guardian-proxy` or the workspace
manifest, so no feature boundary drifted while the hold was in force.

A live `NOT_GA` can never exercise the watch's `GA`-only branch, so the branch was
pinned instead of assumed. Hermetically, the workflow's own extraction and gate
(`sed -n 's/^VERDICT=//p' | tail -1`, empty → `UNKNOWN`, issue step only when
`verdict == 'GA'`) was replayed against fixtures: `GA` takes the issue path, while a
preview description, a prerelease tag, and an unparseable crate payload all stay
log-only, and the retired crate name cannot swing the verdict. Live,
`gh workflow run agt-ga-watch.yml` produced run `35591800534` on `main`
(`9eecf4cd`): `success`, step `Open tracking issue on GA` **skipped**, and the log
carries the same three `source:` lines plus `VERDICT=NOT_GA`. The scheduled run
therefore reports rather than acts while the gate is closed, and no issue was
created (`--search "[agt-ga-watch] in:title"` still returns no match, so the first
`GA` run takes the create branch). The reuse half of that branch was pinned against
live data the same way: the identical pipeline driven by a matching open issue
(`--search "[sensors] in:title"` → `138`) yields a non-empty `--jq` result and takes
the "already open; skipping creation" path, so repeated monthly `GA` runs reuse the
issue rather than filing a new one.

### GA watch run — 2026-09-12

Manual `bash scripts/check-agt-ga.sh`:

- crate: `version=3.2.2 description=Public Preview — Rust SDK for the Agent Governance Toolkit (policy, trust, audit, identity)`
- release: `tag=v4.1.0 prerelease=false`
- `VERDICT=NOT_GA` — criterion (a) still unsatisfied.

### GA watch run — 2026-09-15

Manual `bash scripts/check-agt-ga.sh`:

- crate: `version=3.2.2 description=Public Preview — Rust SDK for the Agent Governance Toolkit (policy, trust, audit, identity)`
- release: `tag=v4.1.0 prerelease=false`
- `VERDICT=NOT_GA` — criterion (a) remains unsatisfied; no promotion review is opened.

### GA watch run — 2026-09-16 (promotion re-review)

Manual `bash scripts/check-agt-ga.sh`:

- crate: `version=3.2.2 description=Public Preview — Rust SDK for the Agent Governance Toolkit (policy, trust, audit, identity)`
- release: `tag=v4.1.0 prerelease=false`
- `VERDICT=NOT_GA` — criterion (a) remains unsatisfied.

The `chore-agt-promotion` gates were re-run in full against this tree to confirm
the hold still reflects reality rather than stale notes. Every engineering gate
passed and criterion (a) still fails, so the fail-closed rule selects `hold`
again: no default-feature or production Rust change was made.

| Gate | Result |
|---|---|
| `bash scripts/check-agt-ga.sh` | `VERDICT=NOT_GA` — criterion (a) unsatisfied (the blocking gate) |
| `cargo check -p guardian-proxy --features agt-governance` | exit 0 |
| `cargo test -p guardian-proxy --features agt-governance` | 31 passed, 0 failed |
| `cargo test -p guardian-proxy` | 30 passed, 0 failed |
| `cargo test -p guardian-proxy --no-default-features` | exit 0 — opt-out contract intact |
| `check-agt` (`do-harness.toml`) | passed |
| `do-harness verify --set verification --strict` | all 12 sensors passed |
| `do-harness status --set verification` | `green` |

`crates/guardian-proxy/Cargo.toml` still declares `default = []`, so the feature
boundary matches this decision. The hold needs no artifact edit; this entry
records that the 2026-09-15 decision was independently reproduced.

## Next action

The `chore-agt-promotion` review is complete as a **hold**, re-confirmed on 2026-09-16, instrument-corrected on 2026-09-19, and re-run unchanged on 2026-09-21. The MCP/tool-call mediation criterion (b) is satisfied behind `mcp-surface`, but AGT criterion (a) remains unsatisfied: `bash scripts/check-agt-ga.sh` reports `VERDICT=NOT_GA` for the **live** `agentmesh` crate, which still describes itself as Public Preview (upstream `CHANGELOG.md`: "All releases are currently **public preview releases**"). Keep `agt-governance` off by default and rerun this review when the GA watch reports `VERDICT=GA`; do not remove the feature flag or alter production Rust until both criteria hold.

When the watch does report `GA`, the promotion work is larger than removing a flag: the pinned dependency is the **retired** `agent-governance` 3.2.2 and the live crate is `agentmesh` 4.x, whose API moved `serde_yaml::Value` to `agentmesh::policy_data::Value` (upstream `YAML-MIGRATION.md`). Promoting therefore means migrating the adapter to `agentmesh` and re-running `cargo deny check` + `cargo audit`, which is also what retires the `RUSTSEC-2026-0097` ignore in `scripts/check-audit.sh`.

## Promotion review — chore-agt-promotion (2026-09-15)

**Decision: hold `agt-governance` off by default.** The authoritative GA gate is not satisfied; no Cargo feature or production Rust change was made.

| Factor | Finding |
|---|---|
| AGT GA criterion (a) | `bash scripts/check-agt-ga.sh` → `VERDICT=NOT_GA`; crates.io reports `agent-governance` 3.2.2 as `Public Preview — Rust SDK for the Agent Governance Toolkit (policy, trust, audit, identity)`; latest toolkit release is `v4.1.0`, non-prerelease, but does not establish SDK GA |
| MCP surface criterion (b) | Satisfied behind optional `mcp-surface`; its default-on decision remains separate |
| Feature-on engineering gates | `cargo check -p guardian-proxy --features agt-governance` passed; `cargo test -p guardian-proxy --features agt-governance` passed with 31 tests and 0 failures; `check-agt` passed |
| Repository verification | `do-harness verify --set verification --format json --strict` passed 12/12; `target/release/do-harness status --set verification` reported `green` |
| Consequence | Default builds keep `agent-governance` absent, preserving the accepted RUSTSEC-2026-0097 scope and pre-GA dependency boundary |


## Slice completion — chore-remove-doharness-policy (2026-09-11, task 30)

Removed the duplicated in-CLI adapter (`crates/do-harness/src/policy/{agt,mcp}.rs`) that had no production caller and carried blanket `#[allow(dead_code)]`. `guardian-proxy` is now the sole tool-call mediation surface and the only home of the feature-gated AGT client (`crates/guardian-proxy/src/proxy.rs` `AgtGateWrapper`). The `agt-governance` feature/optional deps were dropped from `crates/do-harness/Cargo.toml`; CI keeps `cargo check -p guardian-proxy --features agt-governance`. Promotion criterion (a) GA remains unsatisfied (`VERDICT=NOT_GA` above), so the proxy feature flag stays off by default.
