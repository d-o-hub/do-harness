# Epic: Development Signals and the Completion Contract

> **Status:** implementation complete (Issues 1–8)
> **Related:** runtime-neutral signal sets, change-aware applicability,
> evidence freshness, lean adoption contract, DeepSeek Harness bundle
> **Created:** 2026-09-11

## Why

`do-harness` had strong primitives (sensors, JSON reports, retries, evidence,
Git SHA, hash chains) but no answer to: which checks apply to the current
change, whether previous evidence is still valid, and how another runtime
cheaply distinguishes green/red/stale/missing. This epic makes development
signals a first-class, runtime-neutral completion contract.

## Slices

| Slice | Exit criteria | Task |
|-------|---------------|------|
| Signal sets | `[signal-sets]` parsed/validated, `verify --set` runs a set, JSON exposes `signal_set`, legacy configs unchanged | 31 |
| Change-aware applicability | `when-changed` globs, untracked/renamed/deleted discovery, fail-closed on discovery failure, `explain` reasons | 32 |
| Fingerprints + status | workspace/policy fingerprints, evidence schema v3, `status` green/red/stale/missing without running sensors | 33 |
| Lean AGENTS.md | ~50-line routing/completion contract, no mandatory HTN/event/ATDD, non-destructive, development-methodology skills not scaffolded | 34 |
| Evidence-driven init | detects repository, probes tooling, includes only proven signals, executes the baseline, JSON report, red exits non-zero | 35 |
| DSH development_signals | out-of-tree plain-JS bundle, raw typed tool over fixed argv via `ctx.subprocess`, JSON validated, missing binary actionable | 36 |
| DSH completion gate | `agent/turn-stopping` checks cheap `status` only, steers specific reasons, bounded loop safety, fiber-scoped disposal | 37 |
| E2E dogfood + CI | real-binary green→edit→stale→verify→green, red blocking, docs-only selection, CI bundle job | 38 |

## Boundaries

- **do-harness owns:** sensor execution, signal sets, changed-file
  applicability, fingerprints, evidence, freshness, explanations, status.
- **Agent runtime owns:** context injection, sandboxing, process lifecycle,
  permissions, continuation, UI.
- **DSH adapter owns:** the typed tool, `ctx.subprocess` invocation, optional
  completion interception. It contains no verification policy.

## Evidence

- Rust acceptance suites: `tests/signals.rs` (8), `tests/changes.rs` (6),
  `tests/status.rs` (8), `tests/init_bootstrap.rs` (4), dogfood (7).
- JS bundle tests: unit (21) + real-binary e2e (3) under
  `integrations/deepseek-harness/test/`.
- CI: `verify.yml` runs `verify --set verification` + `status` green + a
  separate `dsh-bundle` job.
- Central contract: `green -> edit -> stale -> verify -> green`, proven with
  the real binary (Rust `status.rs` and JS `e2e.test.mjs`).

## Non-goals

- No DeepSeek-specific logic in the Rust core.
- No reimplementation of DSH instructions, skills, sandboxing, or lifecycle.
- No LLM-decided required checks and no full suite on every agent turn.
