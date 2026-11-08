Adds `do-harness job run|status`: bounded unattended execution that proves completion only with the repository's own fresh strict evidence and preserves typed, resumable blocked outcomes.

## Why

#481 asks for unattended work that stays finite and honest: one task, a pinned
revision, a named executor and hard limits; a disposable workspace with a
documented isolation boundary; deterministic repairs only when project policy
declares them; and a stop that is typed, durable, and resumable without renewing
budget or repeating a completed side effect.

## What changed

- `job run --spec <FILE>` creates a disposable git worktree at the resolved
  commit, runs the declared executor under an attempt budget, a whole-job
  deadline, and a per-attempt timeout, and completes only when
  `verify --set <set> --strict --record --unchanged=run` exits 0 and `status`
  reports `green`. An executor exit status, a `done` claim, or a malformed
  result is never proof.
- The executor environment is cleared to a small base set plus the spec's
  `allowed-env`, so protected credentials cannot reach repository code; a token
  budget requires enforceable accounting, and missing authorization blocks with
  an actionable reason instead of widening access.
- Progress is measured from the set of failing and unrunnable sensors, never
  from diagnostic text: repeated and alternating failures stop the job, and
  attempt/time caps apply regardless.
- Stops are typed receipts under `.do-harness/jobs/<id>/`
  (`attempts-exhausted`, `deadline-exhausted`, `no-progress`,
  `verification-failed`, `executor-timeout`, `executor-unavailable`,
  `invalid-executor-output`, `authorization-unavailable`,
  `unsupported-capability`, `ambiguous-requirements`). `--resume` continues the
  persisted budget and start time; a terminal job resumes as an idempotent
  no-op, and a job directory is locked against concurrent runs.
- `[[repairs]]` in `do-harness.toml` declares deterministic mechanical
  corrections. A repair runs only for its failed sensor, at most once per job,
  and only inside the job workspace; it is recorded before it runs.
- A worktree is rejected as the isolation mechanism: the spec must name the
  external process/network/filesystem boundary, which the CLI records and never
  claims to verify.
- `job status` reports receipts read-only. No sensor, hook, or CI path launches
  a job, so deterministic verification never starts a paid model.
- `docs/unattended-jobs.md`, an AGENTS.md entry, and the regenerated CLI docs,
  completions, man page, and `cli-contract.json`.

## Verification

- `cargo test --test job_unattended`: 17/17 pass, covering semantic success,
  executor "done" with failing checks, a declared repair completing without the
  executor, repeated and alternating failures, hung-executor cleanup, malformed
  output, missing credentials (with a parent-only secret asserted absent from
  the executor environment), interruption and resume without renewed budget,
  idempotent resume of a terminal job, dry-run, spawn failure, missing resume
  plan, and the refusal of a worktree as the isolation boundary.
- `cargo test --bin do-harness job::`: 8/8 unit tests pass (spec validation,
  digest/job-id stability, progress classification).
- `cargo clippy -p do-harness --all-targets` is clean; `cargo fmt --all` applied.
- `do-harness loc` reports every new file under 500 LOC (largest 421).
- `docs.py sync --check`, `examples --check`, and `links --check` pass;
  `completions_sync` and `man_sync` pass.
- Full `cargo test --bin do-harness`: 746 pass; 27 `eval::` failures are
  pre-existing in this sandbox and unrelated (the eval fixture reports
  `ModuleNotFoundError: No module named 'yaml'`).
- `magic-numbers` is advisory and already above its blessed baseline on the base
  commit (1905 vs 1719); the shipping code names its constants, and test timing
  literals were converted to named constants.

Closes #481
