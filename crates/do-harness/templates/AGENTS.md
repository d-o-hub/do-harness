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

## Completion

A change is complete only when current verification evidence is green:

    do-harness status --set verification

`status` never runs sensors. `green` means passing evidence matches the
current workspace and policy. `stale` means the workspace or `do-harness.toml`
changed after verification: re-run the verification set. `red` means required
checks failed. `missing` means no current evidence exists yet.

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
- The `loc` sensor enforces the per-file ceiling over `*.rs`; widen it with
  `--root`/`--ext` in `do-harness.toml` (and the matching `when-changed` globs)
  to cover front-end sources with the same invariant.
- Noisy checks can ship with `severity = "warn"` and a `FINDINGS: <n>` output
  marker; `do-harness verify --record --bless` initializes and then pins the
  committed `plans/baselines.json` ratchet (absent until the first bless,
  which means no ceiling), and only ever lowers it.

## Adoption notes

- `do-harness init` writes this file non-destructively; `--force` overwrites.
- Sensors wrap the repository's own scripts to keep local evidence and CI identical.
- The generic pack ships zero sensors: its pass is vacuous, not evidence.
- Git hooks: `do-harness hook install`. CI:
  `do-harness verify --set verification --format json --strict`.
