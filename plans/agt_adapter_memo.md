# AGT Governance Adapter Architecture Decision Memo

## Executive Summary
This tracking spike evaluates the integration of Microsoft's **Agent Governance Toolkit (AGT)** (`microsoft/agent-governance-toolkit`, MIT License, `agent-governance` on crates.io) into `do-harness`.

We conclude that AGT integration must remain **optional**, **compile-time feature-flagged** (`agt-governance`), and **off by default**. It must not be promoted to a default dependency or default execution path until:
1. AGT reaches Generally Available (GA) stability without breaking policy language or SDK API changes.
2. `do-harness` introduces a tool-call mediation surface (e.g., Model Context Protocol / MCP proxy or per-action interceptor).

---

## 1. Context & Crate Analysis

AGT publishes the `agent-governance` (v3.2.2 Public Preview) crate on crates.io. Per AGT documentation:
- **Rust SDK scope**: Implements core governance primitives (`AgentIdentity`, `PolicyEngine`, `TrustManager`, `AuditLogger`, and protection `Ring`s via `AgentMeshClient`).
- **Python SDK scope**: Houses the full stack runtime services.
- **Lifecycle Status**: Public Preview with documented breaking changes across major/minor versions (e.g. v4 policy engine changes/removals in v5).

---

## 2. Architectural Alignment & Design Comparison

`do-harness` and `AGT` operate at distinct boundaries in the agent execution lifecycle:

| Aspect | `do-harness` | AGT (`agent-governance`) |
| :--- | :--- | :--- |
| **Target Surface** | Developer & Agent **dev-loop verification** (builds, tests, linters, invariants, evidence generation). | Per-action **runtime tool-call mediation** (policy check, DID identity, trust score, ring access, hash chain audit). |
| **Execution Phase** | Build-time, pre-commit, pre-push, and task-state transitions. | In-flight per-call tool/action invocation. |
| **Policy Language** | Executable Rust contracts, shell sensors, and `plans/invariants.json`. | Declarative YAML policy engine (`Capability`, `Approval`, `RateLimit`). |

### Key Insight
`do-harness` currently gates code edits and dev-loop steps. It does not currently expose a runtime tool-call mediation surface (such as an MCP gateway or tool-proxy loop). Integrating AGT into `do-harness` dev-loop sensor gates directly would misalign AGT's per-call action mediation design. Therefore, the AGT adapter (`AgtGate`) serves as an optional stub for future runtime mediation surfaces.

---

## 3. Adapter Strategy & Fail-Closed Doctrine

### Feature Flag Design
- Feature flag: `agt-governance = ["dep:agent-governance", "dep:serde_yaml"]` in `crates/do-harness/Cargo.toml`.
- Dependencies are `optional = true`. The default build includes **zero** AGT code or dependencies.

### Fail-Closed Doctrine
In accordance with defense-in-depth principles:
- Any policy-runtime error, missing configuration, or parameter conversion failure in `AgtGate::check` MUST evaluate to **deny** (`Ok(false)`).
- Runtime failures must never default to allow.

---

## 4. Re-evaluation Criteria for Promotion

The `agt-governance` feature flag will be re-evaluated at each upstream AGT release. Promotion out of the feature flag requires meeting all of the following:
1. **Upstream Stability**: AGT achieves GA status with a stabilized policy schema and API contract.
2. **Mediation Surface**: `do-harness` implements an active runtime tool-call proxy or MCP interceptor layer where per-action policy checks apply natively.

---

## 5. Maintenance Invariants
- `cargo check --workspace` (feature off) must remain warning-free and carry zero AGT dependencies.
- CI includes explicit verification of `cargo check -p do-harness --features agt-governance`.
