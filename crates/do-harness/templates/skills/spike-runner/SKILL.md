---
name: spike-runner
description: >
  Execute isolated de-risking spikes for uncertain third-party APIs, novel
  dependencies, or unknown performance boundaries. Use when a task has high
  uncertainty or when HTN decomposition flags an ambiguity. Triggers: "spike",
  "uncertainty", "prototype", "de-risk", "unknown API".
license: MIT
metadata:
  version: "0.1.0"
  tags: spike prototype uncertainty de-risk scratchpad
---

# Spike Runner Skill

## Purpose
Isolate and resolve uncertainty before committing to a vertical slice. A spike
is a throwaway experiment that produces knowledge, not production code.

## When to Spike
- A third-party API or dependency integration is uncertain.
- Performance boundaries are unknown.
- HTN planning flags a `Requires Uncertainty Spike?` branch.

## Execution Steps
1. **Create spike** — isolate the scratchpad in `tests/spikes/` or a temporary
   branch; write the minimal prototype and record the hypothesis.
2. **Execute spike** — run computational verification against the prototype;
   success = exit code 0, no self-assessment.
3. **Record findings** — store the outcome (what worked, what failed, chosen
   approach) in `.do-harness/agent_state.db`.
4. **Clean spike** — remove ephemeral files once findings are recorded.
5. **Transition to slice** — invoke `Vertical Event Slice`
   (`.agents/skills/htn-planner`) with the resolved approach.

## Gotchas
- A spike is not a partial implementation — never advance production code
  through a spike.
- If the spike fails 3 consecutive times, halt and record the error signature.