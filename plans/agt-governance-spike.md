# Decision Memo: Agent Governance Toolkit (AGT) Spike & Adapter

## Context & Background

Microsoft's Agent Governance Toolkit (AGT) publishes the Rust crate `agent-governance` (and `agent-governance-mcp` on crates.io), exposing `AgentMeshClient` with `execute_with_governance()`.
Per the AGT project documentation, language SDKs (including Rust) currently implement core governance functions: policy evaluation, identity, trust, and audit, while the broader management stack is implemented in Python.

AGT is currently in **Public Preview** with documented breaking API and syntax changes across major versions (e.g., policy language changes between releases).

## Technical & Architectural Evaluation

### 1. Crate API Stability
- **Public Preview Status**: `agent-governance` v3.2.x is evolving rapidly. Future releases may alter client initialization parameters, parameter maps, or result structures.
- **Fail-Closed Requirement**: Because governance policy runtime errors must never accidentally permit unverified actions, any integration must handle errors by failing closed (`Ok(false)` or `Err`).

### 2. Architectural Alignment: Dev Loop Harness vs. Per-Call Mediation
- **`do-harness` Scope**: `do-harness` operates primarily as a development-loop harness and computational verification gate (task tracking, HTN planning, computational sensor suites, trace capture, and distillation).
- **AGT Scope**: AGT is designed for per-call, runtime tool-execution governance (e.g. mediating tool invocations in an MCP / tool-calling runtime).
- **Implication**: An AGT governance gate adapter (`AgtGate`) is only operationally relevant once `do-harness` introduces a tool-call mediation surface (e.g., an MCP protocol adapter or dynamic execution proxy).

## Decision & Posture

1. **Feature-Flagged Stub Adapter**:
   - `do-harness` introduces a compile-time gated adapter in `crates/do-harness/src/policy/agt.rs`.
   - Gated behind the `agt-governance` Cargo feature flag in `crates/do-harness/Cargo.toml`.
   - Feature flag is **off by default**.
   - Default builds of `do-harness` carry zero dependencies on `agent-governance`.

2. **Fail-Closed Behavior**:
   - Runtime evaluation or policy processing errors in `AgtGate::check` evaluate to `Ok(false)` (deny action).

3. **CI Verification**:
   - Continuous Integration verifies workspace compilation both with default features (feature off) and with `--features agt-governance` (feature on).
   - Feature-off build remains clean and warning-free.

4. **Promotion Criteria**:
   - The AGT adapter will remain optional and feature-flagged until:
     a) AGT reaches General Availability (GA) stability.
     b) `do-harness` gains an explicit runtime tool-call mediation surface (such as MCP tool call hooks).
   - Re-evaluate adapter stability at each upstream AGT release.
