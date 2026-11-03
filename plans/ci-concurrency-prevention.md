# CI Concurrency & Timeout Prevention Strategy

> **Status:** Active & Enforced
> **Created:** 2026-10-20
> **Target:** CI reliability, OOM prevention, and execution time bounds

## 1. Problem Statement & Context

During CI runs under coverage instrumentation, spawned harness execution runs an order of magnitude slower than uninstrumented builds. When writer waves in concurrent tests pile up without bound (measured on 2026-10-14: past 11GB anonymous memory), the process OOM-kills the coverage run.

Simultaneously, long-running or hung test processes on Windows runners could consume the full 30-minute job timeout budget without emitting output, blocking PR queues and wasting runner compute.

## 2. Prevention Guardrails

To prevent this class of failure across all future runs, the codebase enforces five core guardrails:

1. **Concurrency Wave Throttling Under Coverage**
   - Location: `crates/do-harness/tests/concurrency.rs`
   - Mechanism: Inspects `LLVM_PROFILE_FILE` or `CARGO_LLVM_COV` environment variables.
   - Behavior: Under coverage instrumentation, writer waves reduce from `(2, 3)` to `(1, 1)` to prevent retry storms while continuing to exercise the transactional retry path.

2. **Windows Workspace Suite Sharding & Timeout Bounds**
   - Location: `.github/workflows/verify.yml` (`windows-tests` job)
   - Observed: run #41632894179 hit the old 12-minute step timeout with 750 of 1125 tests done (2 aborted, 373 never run), so the suite exceeded the budget rather than hanging.
   - Structure: three shards (`shard: [1, 2, 3]`) run `cargo nextest run --workspace --release --exclude do-harness-db --partition slice:M/3`. `slice:M/N` is nextest's recommended mode: it lists every test across all binaries and deals them round-robin, so the shards are disjoint, their union is the whole suite, and their sizes stay even. `count:` is deprecated upstream ("neither the even nature of sliced partitioning nor the stability of hashed sharding") and `hash:` can distribute unevenly on small runs.
   - Phases: compilation (`--no-run`) and execution are separate named steps, each bounded at `timeout-minutes: 12`, so the job log separates build time from test time.
   - Diagnostics: `--final-status-level slow` names the slowest tests and `--no-fail-fast` reports every failure in a shard; the step name carries the shard identity.
   - Effect: a hung or slow shard fails in minutes rather than burning the full 30-minute job budget, and a failure in any shard fails the job.
   - Database suite: `do-harness-db` keeps its own serialized `cargo test -p do-harness-db --release -- --test-threads=1` step in the `windows` job and is never folded into the shards.
   - Matrix semantics: `strategy.fail-fast` defaults to `true`, which would cancel the remaining shards on the first failure; the job sets `fail-fast: false` so every shard reports. Job `timeout-minutes` defaults to 360 and step `timeout-minutes` is capped at 360, so the 30-minute job and 12-minute step bounds are both explicit and well inside the platform limits.
   - Sources: [nextest partitioning](https://nexte.st/book/partitioning.html), [nextest reusing builds](https://nexte.st/book/reusing-builds.html), [GitHub Actions workflow syntax](https://docs.github.com/en/actions/reference/workflows-and-actions/workflow-syntax).

3. **Coverage Job Separation**
   - Location: `.github/workflows/scheduled.yml`, `.github/workflows/release.yml`
   - Architecture: Memory-heavy coverage tasks (`cargo llvm-cov nextest`) run in dedicated jobs/workflows with explicit swap space and bounded concurrency (`CARGO_BUILD_JOBS=1`, `NEXTEST_TEST_THREADS=1`), separate from normal Windows or PR test suites.

4. **CI Compiler Memory Bounds**
   - Settings: `CARGO_BUILD_JOBS: "2"` (or `"1"` on heavy coverage steps) and `CARGO_INCREMENTAL: "0"`.
   - Scope: Workflow `env` across `.github/workflows/verify.yml`, `scheduled.yml`, and `release.yml`.

5. **Bounded Timeouts for Test & Coverage Steps**
   - Requirement: Every job or step executing `cargo test`, `cargo nextest`, or `cargo llvm-cov` must declare a `timeout-minutes` bound.

## 3. Automated Validation & Sensors

Compliance is verified automatically on every run via the `ci-concurrency-guardrails` sensor:

- **Check Script:** `scripts/check_ci_concurrency_guardrails.py`
- **Sensor Name:** `ci-concurrency-guardrails` in `do-harness.toml`
- **Self-Test Controls:** Running `python3 scripts/check_ci_concurrency_guardrails.py --self-test` executes negative mutation controls: an over-budget shard timeout, a dropped shard, a partition denominator that disagrees with the matrix, a missing compile timeout, a removed serialized `do-harness-db` step, a compile step that runs tests, a shard without `--no-fail-fast`, a shard on the deprecated `count:` mode, coverage inside the Windows job, and the coverage wave reduction.

## 4. Architecture Invariants

The strategy is recorded as executable invariants in `plans/invariants.json`:

- `ci.windows_test_timeout_bound`
- `ci.windows_shard_coverage`
- `ci.coverage_wave_throttle`
- `ci.coverage_job_separate`
- `ci.build_jobs_bounded`
- `ci.incremental_disabled`
