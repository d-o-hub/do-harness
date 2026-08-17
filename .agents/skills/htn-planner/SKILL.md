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
  version: "1.1"
  tags: htn planning decomposition spike tasks
---

# HTN Planner Skill

## Purpose
Decompose incoming software objectives into deterministic Hierarchical Task Networks (HTN) using methods, subtasks, and precondition guards.

## Method Catalog

### Method: Vertical Event Slice
- **Preconditions**: Domain requirement specified, target entities known.
- **Subtasks**:
  1. `define-event-schema`: Write Command, Event, and State types.
  2. `write-acceptance-test`: Create Red ATDD test fixtures.
  3. `implement-slice`: Write domain logic and event handler.
  4. `verify-sensors`: Run `cargo check`, `cargo test`, and `cargo clippy`.
  5. `distill-learning`: Record heuristics in libSQL.

### Method: Spike & Resolve
- **Preconditions**: High API uncertainty, third-party crate ambiguity, or performance unknown.
- **Subtasks**:
  1. `create-spike`: Generate isolated test scratchpad in `tests/spikes/`.
  2. `execute-spike`: Run computational verification against minimal prototype.
  3. `record-findings`: Store outcome in `.do-harness/agent_state.db`.
  4. `clean-spike`: Remove ephemeral files.
  5. `transition-to-slice`: Invoke `Vertical Event Slice` method.

## Execution Rules
- Check method preconditions before selecting a method; if they are unmet, pick the matching alternative or defer.
- Always persist current method state and subtask index into `plans/tasks.json` or libSQL.
- Never advance subtask pointer until the corresponding sensor passes with exit code 0.
- When a decomposed subtask carries high uncertainty, delegate to `.agents/skills/spike-runner` before advancing.

## Gotchas
- Do not decompose a task that lacks a specified domain requirement — the precondition of `Vertical Event Slice` is unmet.
- A spike decision is made at planning time, not discovered mid-implementation.