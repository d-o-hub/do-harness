# Development contract

This repository uses do-harness for computational verification.

## Prerequisites

The `do-harness` CLI must be on `PATH`, or `DO_HARNESS_BIN` must point at it.
Pin the version this repository was initialized with:

    curl -fsSL https://raw.githubusercontent.com/d-o-hub/do-harness/main/scripts/install.sh \
      | sh -s -- --version v{{VERSION}}

Rust developers can instead build from a checkout with
`cargo install --path <do-harness>/crates/do-harness`.

## Working loop

During implementation:

    do-harness verify --set feedback --changed

Before claiming completion:

    do-harness verify --set verification --changed --strict

Do not disable, bypass, remove, or weaken a required signal merely to obtain a
passing result. Fix the underlying cause and re-run the sensor.

A sensor that runs zero checks is vacuous, not evidence: if `verify` reports
green but a sensor executed nothing (e.g. `0 tests run`), add a real check
before claiming completion.

## Completion

A change is complete only when current verification evidence is green:

    do-harness status --set verification

`status` never runs sensors. `green` means passing evidence matches the
current workspace and policy. `stale` means the workspace or `do-harness.toml`
changed after verification: re-run the verification set. `red` means required
checks failed. `missing` means no current evidence exists yet.

## Main CI and release safety

The repository's post-merge `main` gate should run the complete strict signal
set, including any scheduled or expensive checks that the project treats as
release-blocking. A release workflow must consume a successful gate for the
exact commit it will tag; warnings, skipped sensors, missing tools, and stale
evidence are not release evidence. Automatic repair should be limited to
commands explicitly declared safe by the repository's sensor contract and must
open a pull request. Never push an inferred code, test, security, or dependency
repair directly to `main`. A tag is incomplete until its corresponding GitHub
Release and post-publication checks exist.

## Repository knowledge

- Follow the nearest `AGENTS.md` and the project documentation.
- Load project-specific skills progressively: run
  `do-harness skills suggest --query "<task>" --limit 5` to rank skills by
  frontmatter metadata, inspect the candidate metadata, select the matching
  skill(s), and only then read the selected `SKILL.md` and its references.
  Never read every `SKILL.md` to decide which one applies.
- Sensors and signal sets live in `do-harness.toml`; run
  `do-harness explain --set verification --changed` to see which sensors apply
  to the current change and why.
- If this repository adopts a per-file line ceiling, the Rust pack seeds the
  `loc` sensor over `*.rs`; widen it with `--root`/`--ext` in `do-harness.toml`
  (and the matching `when-changed` globs) for other sources, or remove it.
  Other packs do not ship `loc` by default.
- Beats from `verify --record` scope to the current git branch by default, to
  `--task <id>` when passed, or `--global`. `metrics` filters by that workstream
  so branches do not mix.
- Noisy checks can ship with `severity = "warn"` and a `FINDINGS: <n>` output
  marker; `do-harness verify --record --bless` initializes and then pins the
  committed `plans/baselines.json` ratchet (absent until the first bless,
  which means no ceiling), and only ever lowers it.

## Adoption notes

- `do-harness init` writes this file non-destructively; `--force` overwrites.
- Sensors wrap the repository's own scripts to keep local evidence and CI identical.
- The generic pack ships zero sensors: its pass is vacuous, not evidence.
- Seeded policy, not a universal prerequisite: the 500-LOC `loc` sensor and the
  `scripts/check-commitlint.sh` commit-message check are defaults you may keep,
  replace, or remove. The managed `commit-msg` hook delegates to this
  repository's own script, so the convention is yours to define.
- Git hooks: `do-harness hook install`. CI:
  `do-harness verify --set verification --format json --strict`.
