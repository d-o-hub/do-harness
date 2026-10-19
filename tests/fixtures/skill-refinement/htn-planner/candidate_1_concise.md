---
name: htn-planner
description: >
  Decompose software objectives into Hierarchical Task Networks (HTN) with
  methods, subtasks, and precondition guards.
license: MIT
metadata:
  version: "0.1.0"
---

# HTN Planner Skill

## Method Catalog
- **Vertical Event Slice**: Preconditions: domain requirement specified, target entities known. Subtasks: schema, ATDD test, implementation, sensors (`check`, `test`, `clippy`), distill.
- **Spike & Resolve**: Preconditions: API uncertainty or crate ambiguity. Subtasks: spike in `tests/spikes/`, execute, record in `.do-harness/agent_state.db`, clean, transition to slice.

## Rules
- Check method preconditions before selecting; defer if unmet.
- Persist state to `plans/tasks.json` or libSQL.
- Advance subtask pointer only when sensor passes exit 0.
