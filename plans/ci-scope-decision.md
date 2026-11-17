# CI change-scoped verification: decision

> **Status:** decided 2026-11-16 — keep full PR gating (option 1)
> **Recorded:** `plans/` · **Issue:** #561 · **Measurement:** `plans/ci-scope-measurement.md`

## Decision

The pull-request gate keeps running the full verification plan. No path filter,
no scoped plan job, no change to `pr-required` or the ruleset. The measured
saving is real and is recorded here so a future decision starts from it rather
than re-measuring it.

| Plan | Wall clock | Jobs | Runner-seconds |
| --- | --- | --- | --- |
| full | 14.8 min | 11 | 4,298 |
| scoped (docs only) | 3.8 min | 2 | 217 |

Skipped work was `windows-tests` shards (711+868+873 s), `windows` (322 s),
`source-distribution` (240+377 s), `msrv` (313 s), and `selection-portability`
(47 s). The scoped run stayed green under `--strict` and `pr-required` passed.

## Why not scope

1. **The classifier is the risk, not the saving.** A scoped plan only works
   with a deny-list that keeps Rust, `Cargo.*`, `do-harness.toml`,
   `.github/**`, and `scripts/**` changes on the full plan. A `do-harness.toml`
   or workflow edit is docs-shaped by extension but changes the gate itself;
   an allow-list over prose would mark exactly the changes that can break the
   gate as "docs only". That classifier needs its own can-fail tests (a file
   added in each deny-listed class must still select the full plan) before it
   may gate a merge, and it does not exist yet.
2. **Required-check identity changes.** A scoped plan that skips jobs changes
   which checks report. Branch protection then either requires checks that
   never run (a permanent pending) or stops requiring them (the main-branch
   gate distinguishes itself from `verification` precisely so release
   automation consumes one exact green result). That is a policy change with
   its own failure mode, not a workflow tweak.
3. **The cost is bounded and visible.** The expensive jobs are what make a
   Rust change reviewable on Windows and at MSRV. Paying them on every PR is
   predictable; saving them only on docs-only PRs is a 10-minute optimisation
   that must never be wrong.

## Options considered and rejected

- **Option 2 (scope the whole gate).** Rejected above: no tested deny-list.
- **Option 3 (scope only the expensive jobs).** Same classifier risk with a
  smaller saving; partial scoping still changes which required checks report.
- **Option 4 (pre-push `verify --since @{upstream}`).** Not adopted: the local
  pre-push list is already bounded to the ordinary set, `@{upstream}` is absent
  in a fresh clone (so the run falls back to the full list), and local latency
  is not the measured problem — CI wall clock is.

## Revisit conditions

Adopt scoping only when all of these hold:

- a deny-list classifier exists with can-fail tests for Rust, `Cargo.*`,
  `do-harness.toml`, `.github/**`, `scripts/**`, and a new top-level directory;
- scoped runs still use `--set verification --strict` and `pr-required`
  semantics are unchanged in the ruleset;
- a second measurement on the classifier-instrumented workflow confirms the
  saving, since the 2026-11-09 prototype predates the classifier tests.

Until then the answer is the full gate, and the decision lives here rather than
in an open issue.
