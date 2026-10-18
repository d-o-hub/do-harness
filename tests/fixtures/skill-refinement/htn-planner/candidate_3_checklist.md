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

| Method | Preconditions | Subtask Sequence | Sensor Gate |
|---|---|---|---|
| **Vertical Event Slice** | Domain requirement specified, target entities known | 1. `define-event-schema`<br>2. `write-acceptance-test`<br>3. `implement-slice`<br>4. `verify-sensors`<br>5. `distill-learning` | `check`, `test`, `clippy` |
| **Spike & Resolve** | High API uncertainty, third-party crate ambiguity | 1. `create-spike` in `tests/spikes/`<br>2. `execute-spike`<br>3. `record-findings` in `.do-harness/agent_state.db`<br>4. `clean-spike`<br>5. `transition-to-slice` | Exit code 0 |

## Precondition & Transition Protocol Checklist
- [ ] Evaluate preconditions before method selection. If domain requirement missing -> defer, no method applies.
- [ ] Persist current method state and subtask index into `plans/tasks.json` or libSQL database.
- [ ] Subtask pointer advance blocked until sensor gate passes with exit code 0.
