---
name: htn-planner
description: >
  Decompose software objectives into deterministic Hierarchical Task Networks
  (HTN) with methods, subtasks, and precondition guards. Use when planning a
  compound coding task, breaking a feature into ordered subtasks, choosing
  between a vertical event slice and a de-risking spike, or persisting task
  state to plans/tasks.json or libSQL.
license: MIT
metadata:
  version: "0.1.0"
  tags: htn planning decomposition spike tasks
---

# HTN Planner Skill (Comprehensive Guidance & Architecture Manual)

## Core Philosophy
The Hierarchical Task Network (HTN) planner provides a structured approach to breaking down software engineering challenges into manageable, deterministic execution blocks. Software engineering tasks often fail due to unexpected complexity or unverified assumptions; HTN planning mitigates this risk by enforcing strict method preconditions and explicit sensor-gated subtask transitions.

## Method Catalog & Execution Details

### Method 1: Vertical Event Slice
- **Purpose**: Ideal for feature implementation where domain model boundaries and requirements are well-understood.
- **Preconditions**:
  1. A clear domain requirement is specified in the prompt/task description.
  2. Target entities and state transitions are known beforehand.
- **Ordered Subtask Sequence**:
  1. `define-event-schema`: Define explicit data structures for Commands, Events, and State.
  2. `write-acceptance-test`: Create failing (Red) ATDD test fixtures to define expected behavior.
  3. `implement-slice`: Write domain logic and event handlers to fulfill the acceptance test.
  4. `verify-sensors`: Run verification suite (`cargo check`, `cargo test`, `cargo clippy`).
  5. `distill-learning`: Record heuristics into libSQL database for downstream task learning.

### Method 2: Spike & Resolve
- **Purpose**: Used when facing high API uncertainty, unverified third-party dependencies, or unknown performance characteristics.
- **Preconditions**:
  1. High API uncertainty or unproven library interface.
  2. Subtask decomposition cannot be completed without empirical verification.
- **Ordered Subtask Sequence**:
  1. `create-spike`: Generate an isolated test scratchpad in `tests/spikes/`.
  2. `execute-spike`: Execute minimal computational prototype to test assumptions.
  3. `record-findings`: Store outcome in `.do-harness/agent_state.db`.
  4. `clean-spike`: Clean up ephemeral scratch files.
  5. `transition-to-slice`: Transition to `Vertical Event Slice` with proven assumptions.

## Mandatory Execution Protocols
- Preconditions must be evaluated prior to method selection. If preconditions for a method are not satisfied (e.g., domain requirements missing for a vertical slice), do not select that method. Defer execution or ask for clarification.
- State persistence is strictly enforced: every task transition must update `plans/tasks.json` or `agent_state.db`.
- Subtask pointers must never advance unless all mandatory sensor gates yield exit code 0.
