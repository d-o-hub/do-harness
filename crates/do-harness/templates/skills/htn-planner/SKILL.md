---
name: htn-planner
description: >
  Decompose software objectives into deterministic Hierarchical Task Networks
  (HTN) with methods, subtasks, and precondition guards. Use when planning a
  compound coding task, breaking a feature into ordered subtasks, or choosing
  between a vertical event slice and a de-risking spike.
license: MIT
metadata:
  version: "0.1.0"
  tags: htn planning decomposition tasks
---

# HTN Planner Skill

## Purpose
Decompose incoming objectives into deterministic Hierarchical Task Networks
using methods, subtasks, and precondition guards.

## Method Catalog

### Method: Vertical Event Slice
- **Preconditions**: domain requirement specified, target entities known.
- **Subtasks**:
  1. Define the command/event/state schema (`.agents/skills/event-modeler`).
  2. Write failing acceptance fixtures (ATDD red).
  3. Implement the slice (ATDD green).
  4. Run `do-harness verify`; every sensor must exit 0.
  5. Distill learnings (`.agents/skills/skill-distiller`).

### Method: Spike & Resolve
- **Preconditions**: high API/dependency uncertainty or unknown boundaries.
- **Subtasks**:
  1. Create an isolated scratchpad (see `.agents/skills/spike-runner`).
  2. Run computational verification against the prototype.
  3. Record findings in `.do-harness/agent_state.db`.
  4. Clean up ephemeral files.
  5. Transition to `Vertical Event Slice` with the resolved approach.

## Execution Rules
- Check method preconditions before selecting a method; if unmet, pick the
  matching alternative or defer.
- Persist task state in `plans/tasks.json` or the local state database.
- Never advance a subtask until its sensor passes with exit code 0.

## Gotchas
- Do not decompose a task that lacks a specified requirement — the
  precondition of `Vertical Event Slice` is unmet.
- A spike decision is made at planning time, not discovered mid-implementation.