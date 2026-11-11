# `do-harness sync` — external adopter validation

> **Status:** recorded - external adopter validation of `sync` at v0.1.49
> **Recorded:** `plans/`

Validation of the `sync` command shipped in #511 against a repository that is a
real harness adopter rather than this one: `d-o-hub/rust-self-learning-memory`
at `a190a301` (v0.1.49), cloned read-only. No ref was pushed to that repository;
that checkout's `origin/main` was verified unchanged afterwards.

## Why that repository

It carries its own `do-harness.toml`, a tracked version contract
(`scripts/do-harness-version.txt`, `pin = 0.1.0`, `validated_through = 0.4.1`),
and a compatibility probe (`scripts/check-do-harness-compat.sh`) that asserts the
CLI surface, signal-set parity, sensor paths, exit-code doctrine, and `doctor`.
That makes it an independent judge of the binary rather than a second copy of
this repository's own assumptions.

## Compatibility probe

Run as their CI does — binary installed outside any workspace `target/`, with
`DO_HARNESS_BIN` set (`harness-compat` job in `.github/workflows/skill-evals.yml`):

<!-- example: id=sync_adopter_compat classification=historical reason="Recorded output of a foreign repository probe on 2026-11-09, not reproducible from this repository" -->
```text
OK: binary 0.4.2 satisfies the tracked pin 0.1.0
NOTE: binary 0.4.2 is newer than validated_through 0.4.1; raise the ceiling only on a measured compat run
OK: AGENTS.md pins 0.1.0, equal to scripts/do-harness-version.txt
OK: subcommand surface intact: verify status explain list doctor version
OK: verify offers every flag the contract cites: --set --only --strict --record --format --exclude --config --root
OK: every signal-set member is a defined sensor
OK: set 'feedback' resolves to the 5 sensors declared in do-harness.toml
OK: set 'verification' resolves to the 23 sensors declared in do-harness.toml
OK: every sensor argv path exists
OK: doctor (non-strict) runs against this workspace
=== do-harness CLI Compatibility Passed (0.4.2) ===
```

Two probe notes, neither a defect in `sync`:

- The probe correctly warns that `0.4.2` is newer than their `validated_through`
  0.4.1. Raising that ceiling is a deliberate change on *their* side, requiring a
  recorded run — this document is the evidence such a PR would cite.
- An earlier run failed the `doctor` step with `binary missing at
  <workspace>/target/release/do-harness`. That is `doctor`'s documented
  repo-local fallback (`BinSource::Repo`) when no binary is on `PATH`, not a
  `sync` behaviour; installing the binary the way their CI does removes it.

## Sync scenarios

Each row was executed against the clone. "Untouched" means the described ref,
`HEAD`, or worktree was re-read after the command.

| # | Scenario | Expected | Observed |
|---|---|---|---|
| 1 | `--read-only` with upstream reachable | advertised SHA reported, no ref written | state `read_only`, `a190a30`, tracking ref unchanged |
| 2 | Default on clean, current `main` | no-op | state `up_to_date`, `ahead 0`, `behind 0`, `aligned true` |
| 3 | Untracked file present, current `main` | not refused; report it | `up_to_date`, `clean true`, `untracked true` |
| 4 | Tracked change present, current `main` | no fast-forward, refuse nothing destructively | `up_to_date`, `clean false`; tree preserved |
| 5 | Feature branch, no `--switch` | fetch only, stay put | state `fetched`, branch stayed `probe-feature` |
| 6 | Local `main` rewound one commit | fast-forward back to upstream | `fast-forwarded main to origin/main (a190a30)` |
| 7 | Local commit on `main` (diverged) | exit 1, move nothing | `refused_diverged`, `ahead 1`, `HEAD` unchanged, local subject listed |
| 8 | `--reset-diverged` on that divergence | backup created, `main` reset exactly | backup `backup/main-pre-sync-dcddcc7` held the pre-reset SHA; `main` == `origin/main` |
| 9 | `--switch` from a feature branch | land on `main` | `switched from probe-switch`, branch `main` |
| 10 | No `origin` remote | exit 2, nothing written | exit 2, `remote origin is not configured` |

Scenario 7 and 8 use the pair that matters most: the refusal left the local
commit in place and reported it, and the explicit recovery preserved it as a
named backup before moving `main`. Scenario 3 and 4 are the pair that exposed
the pre-merge defect (untracked files previously refused as "dirty"); they now
behave as designed on a real 1,200-commit history with submodules and generated
trees, not only on the synthetic bare-remote fixtures.

## Residual risk

- Validation used `0.4.2` built from this branch, not a published release, so the
  probe's `validated_through` note stays open until a release carries `sync`.
- Their `pin = 0.1.0` predates `sync`; adopters on the pin do not get the command
  and keep using `latest-main.sh` — which is exactly what the bootstrap fallback
  in this repository's own guidance is for.
- Only Linux x86_64 was exercised. `sync` shells out to `git` and compares
  object ids as text, so no platform-specific path is expected, but that is
  reasoning, not a measured result.
