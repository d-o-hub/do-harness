---
name: skill-distiller
description: >
  Compress successful interaction traces, compiler error-recovery patterns, and
  spike learnings into reusable, benchmarked skills. Use after a vertical slice
  passes all computational sensors, after recovering from a non-trivial error,
  when the steering loop requires updating a feedforward guide, or when
  creating or updating a skill in .agents/skills/.
license: MIT
metadata:
  version: "0.1.0"
  tags: distillation dreaming skills heuristics eval steering
---
## Guides

See references/heuristics.md for distilled heuristics.


# Skill Distiller & Dreaming Skill

## Purpose
Post-task distillation loop that compresses successful interaction traces, compiler error-recovery patterns, and spike learnings into reusable, benchmarked skills.

## Distillation Trigger
Execute when:
1. A vertical slice successfully passes all computational sensors (`cargo test`, `cargo clippy`).
2. An agent recovered from a non-trivial error (e.g., borrow checker conflicts, async lifetime issues, libSQL driver quirks).
3. A spike resolved a previously unknown architectural constraint.
4. **Steering loop**: the same sensor fired more than 2 times in one sprint — the root cause should become a feedforward guide.

## Execution Steps

### 1. Extract Interaction Trace
- Query `.do-harness/agent_state.db` for the active session's commands, error diffs, and resolution steps.

### 2. Generalize Heuristic
- Convert the specific fix into a generalized pattern.
- Strip project-specific identifiers while preserving the structural solution.

### 3. Update / Create Agent Skill
- If a skill in `.agents/skills/` matches the domain, update its `SKILL.md` with the new heuristic.
- If novel, generate a new directory `.agents/skills/<skill-name>/` with:
  - `SKILL.md`: Core guidance, invariants, and code examples (with Agent Skills frontmatter).
  - `evals/evals.json`: At least 2-3 verification cases with `id`, `prompt`, `expected_output`, and checkable `assertions`.

### 4. Benchmark & Evaluate
Run the skill-evaluator loop:
1. **Structure check** — `SKILL.md` present, frontmatter valid, `evals/evals.json` exists and parses.
2. **Eval review** — each case has a real prompt, a concrete expected outcome, and checkable assertions.
3. **Live run** — execute one representative prompt against the skill and grade with evidence.
4. **Baseline comparison** — when measuring improvement, rerun the same prompt without the skill or against the previous version.
5. **Verdict** — `PASS`, `NEEDS_WORK`, or `FAIL` with evidence; iterate until `PASS`.

### 5. Steering Loop
- When a sensor fires repeatedly (>2 times in one sprint), update the matching feedforward guide (AGENTS.md or the relevant skill) instead of patching symptoms.
- If no guide exists, create one in `.agents/skills/` via this skill's steps 1-4.
- The loop closes the harness: sensors fire -> guides update -> sensors fire less.

## Gotchas
- Never distill a fix that did not pass computational sensors — hallucinations propagate.
- Strip secrets and machine-specific paths from traces before writing them into a skill.