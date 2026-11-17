# PR-time coverage ratchet: decision

> **Status:** decided 2026-11-16 — keep coverage in the scheduled/release gates (option 3)
> **Recorded:** `plans/` · **Issue:** #567

## Decision

`coverage` stays a `warn`-severity sensor in the `scheduled` and `release`
signal sets only. It does not join `verification`/`main` and no PR-time ratchet
is added. The existing building blocks (`plans/baselines.json` findings ratchet,
`pr waivers --lcov`) are unchanged.

## Cost estimate

Measured inputs from `plans/ci-workflow-costs.json` and the coverage notes in
`AGENTS.md`:

| Item | Cost |
| --- | --- |
| `verify` scheduled job (powerset + coverage, bounded parallelism) | 660 s wall, cached |
| Instrumented suite peak memory | documented ~11 GB OOM amplifier; hosted runners need a 4 GB swap file |
| Per-PR coverage run (option 2, patch coverage) | ~11 min added to every Rust PR, with `CARGO_BUILD_JOBS=1` and single-threaded nextest |
| 40 Rust PRs/month | ~7.3 additional runner-hours/month, before cache misses |
| Recorded-baseline check (option 1) | seconds to read a row, but the baseline must first be produced by a full instrumented run |

## Why not a PR-time ratchet

1. **A line-coverage baseline is selection-sensitive.** `--changed`/`--since`
   select sensors from the diff; a docs-only or config-only PR produces no
   coverage observation at all, so a ratchet has to define what "not measured"
   means without turning it into a pass. That is the same vacuous-evidence
   problem `verify` already refuses for sensors.
2. **Patch coverage needs a classifier of acceptable residue.** `pr waivers
   --lcov` exists, but failing on every uncovered added line makes generated
   code, error-only branches, and unreachable defensive arms blocking. The
   waivable classes are not defined, so the gate would either be noisy or
   silently narrow.
3. **The failure already lands before a tag.** Coverage runs nightly and in the
   release gate; a regression is caught within a day, before release, without
   paying the instrumented suite on every PR.

## Revisit conditions

- the instrumented suite becomes shardable well below the memory ceiling, so a
  PR run costs single-digit minutes;
- waivable residue classes are defined and tested (`pr waivers --lcov` refuses
  an unknown class);
- a recorded-baseline check can prove it only lowers through `--bless` with a
  recorded approver, and reports "not measured" as red rather than green.
