---
name: spike-runner
description: >
  Execute isolated de-risking spikes for uncertain third-party APIs, novel
  crate integrations, or unknown performance boundaries. Use when a task has
  high uncertainty, when HTN decomposition flags an ambiguity, or when a
  minimal prototype must pass with exit code 0 before transitioning to a
  vertical event slice. Triggers: "spike", "uncertainty", "prototype",
  "de-risk", "unknown API".
license: MIT
metadata:
  version: "0.1.0"
  tags: spike prototype uncertainty de-risk scratchpad libsql
---

# Spike Runner Skill

## Purpose
Isolate and resolve uncertainty before committing to a vertical slice. A spike is a throwaway experiment that produces knowledge, not production code.

## When to Spike
- Third-party API or crate integration is uncertain.
- Performance boundaries are unknown.
- A novel pattern (async, serde, libSQL) needs validation.
- HTN planning flags `Requires Uncertainty Spike?`.

## Execution Steps

### 1. Create Spike
- Generate an isolated scratchpad in `tests/spikes/` (or a temporary branch).
- Write the minimal prototype: the single uncertain thing, and nothing else.
- Record the spike's hypothesis before writing code.

### 2. Execute Spike
- Run computational verification against the prototype:
  ```bash
  cargo check
  cargo test
  ```
- Success = exit code 0. The prototype must compile and run; no LLM self-assessment.

### 3. Record Findings
- Store the outcome in `.do-harness/agent_state.db` (heuristics / traces tables): what worked, what failed, the chosen approach, and error signatures.

### 4. Clean Spike
- Remove ephemeral files and scratch code once findings are recorded.
- Do not let spike code leak into production slices.

### 5. Transition to Slice
- Invoke the `Vertical Event Slice` method (see `.agents/skills/htn-planner`) with the resolved approach.
- Update the parent task's state in `plans/tasks.json` or libSQL.

## Gotchas
- A spike is not a partial implementation — never advance production code through a spike.
- If the spike fails 3 consecutive times, halt and record the error signature (fail-fast policy).