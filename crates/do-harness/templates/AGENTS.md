# Development contract

This repository uses do-harness for computational verification.

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
- Load project-specific skills from `.agents/skills/` only when applicable.
- Sensors and signal sets live in `do-harness.toml`; run
  `do-harness explain --set verification --changed` to see which sensors apply
  to the current change and why.

## Adoption notes

- `do-harness init` writes this file non-destructively; `--force` overwrites.
- The generic pack ships zero sensors: its pass is vacuous, not evidence.
- Git hooks: `do-harness hook install`. CI:
  `do-harness verify --set verification --format json --strict`.
