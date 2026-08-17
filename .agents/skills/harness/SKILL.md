---
name: harness
description: >
  Map the harness-engineering feedforward guides and feedback sensors, and run
  the self-correction protocol when a computational sensor fires. Use when a
  sensor fails (cargo check, test, clippy, fmt), before making code changes, or
  when setting up agent context for a new task. Triggers: "harness", "sensor
  fire", "CI failure", "self-correction", "swarm".
license: MIT
metadata:
  version: "0.1.0"
  tags: harness sensors feedback feedforward self-correction quality
---

# Harness Skill

## What Is the Harness

Agent = Model + Harness. The harness is the system of feedforward guides (what to do before coding) and feedback sensors (what catches violations after coding).

- **Feedforward (guides):** context, constraints, conventions that prevent errors before they happen.
- **Feedback (sensors):** automated checks that fire after code changes, providing structured error output.

Two modes:
- **Computational:** deterministic checks (fmt, clippy, test) — always trust the output.
- **Inferential:** LLM-based guidance (skill docs, agent context) — direction, not commands.

## Feedforward Guides

| Guide | Path | Purpose |
|-------|------|---------|
| Agent contract | `AGENTS.md` | Operating invariants and 6-phase workflow |
| HTN planning | `.agents/skills/htn-planner` | Task decomposition |
| Event modeling | `.agents/skills/event-modeler` | Event-slice schemas |
| Spike running | `.agents/skills/spike-runner` | De-risking spikes |
| Distillation | `.agents/skills/skill-distiller` | Distillation loop |

## Feedback Sensors

| Sensor | Command | Stage |
|--------|---------|-------|
| verify | `do-harness verify` | pre-commit (subset) + pre-push + CI (full) |
| fmt | `do-harness verify --only fmt` | pre-commit |
| loc | `do-harness verify --only loc` | pre-commit |
| check | `do-harness verify --only check` | pre-push + CI |
| clippy | `do-harness verify --only clippy` | pre-push + CI |
| test | `do-harness verify --only test` | pre-push + CI |
| deps | `do-harness verify --only deps` | pre-push + CI |
| migrate | `cargo run -p do-harness-db --bin init_db` | on schema change |
| seed | `cargo run -p do-harness-db --bin seed_invariants` | when `plans/invariants.json` changes |

## Self-Correction Protocol

When a computational sensor fires:
1. Read the full error message — it includes a fix hint.
2. Classify the error: fmt / check / lint / test / schema.
3. Apply the minimal fix — do not refactor unrelated code.
4. Re-run the specific sensor.
5. Only proceed when the sensor is green.
6. Write a metrics event to `.agents/events/YYYY/MM/DD/` if the fix was non-trivial.

## Fail-Fast Policy

If the same subtask fails a sensor 3 consecutive times: halt, record the error signature in `.do-harness/agent_state.db`, and surface a diagnostic to the developer.

## Steering Loop

When any sensor fires repeatedly (>2 times in one sprint):
1. Identify the root cause category (maintainability / architecture / behaviour).
2. Update the corresponding feedforward guide to prevent recurrence.
3. If no guide exists, create one in `.agents/skills/` using the `skill-distiller` skill.
4. The loop closes: sensors fire -> guides update -> sensors fire less.

## Swarm Handoff Protocol

Handoffs between parallel swarm agents are accepted only on computational evidence: the receiving agent verifies the expected artifacts exist (files, directories, DB rows), and the handing-off agent reports exact verification outputs (commands run, exit codes).

- An empty agent result means the work was not done — treat it as a failed handoff, not a success.
- On failed handoff: take over the work directly, record the pattern in this skill, and log it in the metrics event for the day.
- Parallel swarm agents must own non-overlapping file sets to avoid edit conflicts.
- Distill the failure pattern back into this skill so the sensor (empty-result handoff) fires less in the future.

## New-Repo Adoption (Dogfood Rule)

Using the harness in another codebase is proven, never assumed:

- `do-harness init` (rust pack) scaffolds a minimal cargo crate when no
  `Cargo.toml` exists, so `init && verify` exits 0 on a truly empty tree;
  existing crates are never touched, not even with `--force`.
- The generic pack ships zero sensors: its `verify` pass is vacuous until
  real `[[sensors]]` are configured — do not report it as evidence.
- The green/red paths are dogfooded by `crates/do-harness/tests/dogfood.rs`
  and CI (`init && verify` on a fresh temp workspace every push); re-prove
  with `do-harness verify --format json` in the consumer repo.

## Gotchas
- Never trust LLM self-assessment over a computational sensor's exit code.
- Fix the sensor that fired; do not refactor unrelated code in the same pass.
- An empty sensor suite passes vacuously; that is not evidence.