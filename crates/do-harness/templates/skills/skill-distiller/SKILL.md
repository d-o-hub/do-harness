---
name: skill-distiller
description: >
  Compress successful interaction traces, error-recovery patterns, and spike
  learnings into reusable, benchmarked skills. Use after a vertical slice
  passes all computational sensors or when the steering loop requires updating
  a feedforward guide.
license: MIT
metadata:
  version: "0.1.0"
  tags: distillation dreaming skills heuristics steering
---

# Skill Distiller & Dreaming Skill

## Purpose
Post-task distillation loop that compresses successful traces, error-recovery
patterns, and spike learnings into reusable skills.

## Distillation Trigger
Execute when:
1. A vertical slice passes all computational sensors.
2. An agent recovered from a non-trivial error.
3. A spike resolved a previously unknown constraint.
4. The same sensor fired more than 2 times in one sprint — the root cause
   should become a feedforward guide.

## Execution Steps
1. **Extract** — query `.do-harness/agent_state.db` for the session's
   commands, error diffs, and resolution steps.
2. **Generalize** — convert the specific fix into a structural pattern,
   stripping project-specific identifiers.
3. **Update/create skill** — update the matching `SKILL.md` or create a new
   `.agents/skills/<name>/` with `SKILL.md` and `evals/evals.json`.
4. **Evaluate** — structure check, eval review, and a live run; verdict
   `PASS` / `NEEDS_WORK` / `FAIL` with evidence; iterate until `PASS`.
5. **Steering loop** — when a sensor fires repeatedly, update the matching
   feedforward guide so sensors fire less over time.

## Gotchas
- Never distill a fix that did not pass computational sensors —
  hallucinations propagate.
- Strip secrets and machine-specific paths from traces before writing them
  into a skill.