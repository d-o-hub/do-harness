# AGENTS.md — Agent Execution Harness Contract

This workspace is governed by the do-harness agent execution harness. Agents
and CI must pass the computational sensors before declaring work complete.

## Invariants
- Computational sensors (`do-harness verify`) strictly supersede LLM
  self-assessment: a task is complete only when verified by automated exit
  codes.
- Task tracking lives in `plans/tasks.json` or the local state database
  (`.do-harness/agent_state.db`).
- All skills reside in `.agents/skills/`; task tracking resides in `plans/`.

## Greenfield Adoption (No Assumptions)
- `do-harness init` (rust pack) also scaffolds a minimal cargo crate when no
  `Cargo.toml` exists, so `init && verify` is green on an empty tree;
  existing crates are never touched, not even with `--force`.
- The generic pack ships no sensors: `verify` exits 0 without running any
  command — a vacuous pass. Add `[[sensors]]` before treating it as evidence.
- Never assume the harness works in this repo: re-prove it with
  `do-harness verify --format json` and read the per-sensor exit codes.

## 6-Phase Coding Workflow
1. **HTN planning** — decompose the request into ordered subtasks
   (`.agents/skills/htn-planner`).
2. **Spike** — if an API, dependency, or boundary is uncertain, isolate it in
   a throwaway scratchpad first (`.agents/skills/spike-runner`).
3. **Event modeling** — define commands, events, and projections before
   implementation (`.agents/skills/event-modeler`).
4. **ATDD red** — write failing acceptance fixtures first.
5. **Implement & verify** — make the fixtures pass, then run
   `do-harness verify`; every sensor must exit 0.
6. **Distill** — compress what you learned into the matching skill
   (`.agents/skills/skill-distiller`).

## Self-Correction Protocol
- On a sensor failure: classify it, apply the minimal fix, re-run the failing
  sensor, and proceed only when it is green.
- If the same subtask fails the same sensor 3 consecutive times, halt and
  surface a diagnostic (fail-fast policy).

## Workflow Gates
- Pre-commit and pre-push: `do-harness verify --fail-fast` (via
  `do-harness hook install`)
- CI: `do-harness verify --format json --evidence .do-harness/evidence.json --strict` (exit 0 = pass, 1 = sensor failure / weak evidence,
  2 = usage/config error)

Sensors are defined in `do-harness.toml`; `do-harness hook install` wires the
git hooks, and `do-harness init-db` / `seed` initialize local state.