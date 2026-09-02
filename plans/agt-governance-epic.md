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

## Epic slices (HTN methods, not yet implemented)

| Slice | Autonomy | Exit criteria | Source |
|-------|----------|---------------|--------|
| `spike-agt-api-stability` | spike-runner | `plans/agt-governance-epic.md` updated with AGT `3.x→4.x→5.x` API diff; no code | #14 spike tasks |
| `feat-agt-adapter` | vertical slice | `policy/agt.rs` + `policy/mod.rs` restored with `#[cfg(feature="agt-governance")]`, `Cargo.toml` feature, `verify.yml` feature-on check | #25/#30 diff |
| `feat-agt-mcp-surface` | vertical slice | MCP tool-call hook exists so `AgtGate::check` is wired, not dead code | #14 "mediation surface" |
| `chore-agt-promotion` | decision | GA + surface satisfied → remove feature flag or keep off-by-default per invariants review | Decision memo §4 |

## Non-goals

- No runtime dependency on `agent-governance` without the flag.
- No Python; Rust/TOML/YAML/SQL only.
- No over-claiming compliance (OWASP/NIST/EU AI Act/SOC2) — see `docs/compliance.md` for honest scope (not reproduced here).

## Swarm orchestration

- **Orchestrator** decomposes slices into `plans/tasks.json` / `do-harness-db`.
- **Agents:** `issue-triage`, `pr-impact`, `roast-critic`, `closer-gate` (full autonomy as requested — closer closes no-impact PRs after cherry-pick, no human gate).
- **Sensors:** `fmt`, `loc (500 ceiling)`, `deps (types→no storage)`, `check`, `clippy -D warnings`, `test`, `audit`, `commitlint` (`do-harness verify`).

## Verification checklist

- [ ] `cargo check --workspace` (feature off) clean — currently true on `origin/main` (`95fe3ec`)
- [ ] `cargo check --workspace --features agt-governance` clean — only after `feat-agt-adapter` slice lands
- [ ] `plans/agt-governance-epic.md` stays in sync with `crates/do-harness/src/policy/agt.rs` when adapter exists
- [ ] SHA pin `c57d9d9a...` stays (`#31`), no tag reversion

## Next action

No code in this PR — epic is documentation-only. When AGT releases GA or MCP surface lands, advance `feat-agt-adapter` via a fresh vertical slice, referencing this epic and #14 for GWT contracts.
