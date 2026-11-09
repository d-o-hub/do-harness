# CI scoping measurement — 2026-11-09

Measured on `d-o-hub/do-harness` with a throwaway prototype: a `scope` job that
classifies the pull request with `scripts/ci_scope.py`, then lets five platform
jobs skip when the change cannot affect them. Both runs used the same prototype
commit; the only variable was whether the diff touched Rust.

## Results

| Plan | Run | Wall clock | Jobs | Runner-seconds |
| --- | --- | --- | --- | --- |
| `full` | [42658648253](https://github.com/d-o-hub/do-harness/actions/runs/42658648253) | 891 s (**14.8 min**) | 11 | 4,298 |
| `scoped` | [42657883437](https://github.com/d-o-hub/do-harness/actions/runs/42657883437) | 230 s (**3.8 min**) | 2 | 217 |

Wall-clock saved: **661 s (74%)**. Runner-seconds saved: **4,079 (95%)**.

`full` began 16:49:36 and finished 17:04:27; `scoped` began 16:44:58 and
finished 16:48:48.

## Per-job detail — full plan

| Job | Queue | Run |
| --- | --- | --- |
| `scope` | 2 s | 5 s |
| `msrv` | 3 s | 313 s |
| `source-distribution` (linux) | 3 s | 240 s |
| `windows` | 3 s | 322 s |
| `windows-tests (3)` | 3 s | 873 s |
| `windows-tests (2)` | 4 s | 868 s |
| `windows-tests (1)` | 4 s | 711 s |
| `verify` | 4 s | 539 s |
| `source-distribution` (windows) | 4 s | 377 s |
| `selection-portability` | 4 s | 47 s |
| `pr-required` | 2 s | 3 s |

Skipped in the scoped plan: 3,751 s of the 4,079 s saved. The remainder is the
shorter `verify` job (213 s versus 539 s). A skipped job reports `-1` seconds
from the API, so the per-job `scoped` sum (214 s) understates the two jobs that
actually ran (217 s); the table uses the run's wall clock throughout.

## Selection evidence

`verify --set verification --since <base>` on the real Python-and-docs merge
`ed0ab32` selected 11 of 36 sensors; on the Rust merge `045afa7` it selected 24.
The scoped CI run logged `scoped run against base 26a8832…` and finished
`All sensors passed.` under `--strict`, while `pr-required` reported success for
the skipped dependencies.

The classifier independently answered `scoped` from the real pull-request diff
(`REASON: all 1 changed path(s) are non-platform`), and answered `full` for the
control PR that touched `crates/db/src/lib.rs`
(`REASON: 1 of 2 path(s) outside the non-platform allowlist`).

## Caveat — queue time dominates

This window had 3–4 s queues. An earlier pull request in the same repository
waited 13–18 minutes per job before running, driven by the account's concurrent
runner limit, and `windows-tests (3)` alone queued 1,060 s. Scoping removes nine
jobs from that queue, so its benefit on a saturated pool is larger than the 74%
above — but the number here is the *measured* case, not a guarantee.

## What was not proven

- **No policy change.** `pr-required` and the `main` ruleset
  (`plans/pr-merge-policy.toml`) are untouched. Making required checks
  change-sensitive is a separate decision.
- **Docs-only scoping is safe; toml-only scoping is not.** `docs/**` cannot
  affect the compiled binary, but `do-harness.toml` looks like configuration and
  *is* the gate. The classifier deny-lists it, so a sensor-config edit always
  runs the full matrix.
- **One flake in the first attempt.** The initial measurement run failed on
  `commitlint`, because the scratch commit subject `scratch(docs): …` is not a
  conventional type. That was the sensor rejecting its own bad input, not a
  prototype defect; the corrected subject is the green run above.
