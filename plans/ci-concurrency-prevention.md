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

2. **Windows CI Test Step Timeout Bounds**
   - Location: `.github/workflows/verify.yml`
   - Bound: `timeout-minutes: 12` on the Windows workspace test step.
   - Effect: A hung Windows test fails fast in minutes rather than burning the full 30-minute job budget.

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
- **Self-Test Controls:** Running `python3 scripts/check_ci_concurrency_guardrails.py --self-test` executes negative mutation controls (verifying that removing timeouts or wave reduction triggers non-zero failures).

## 4. Architecture Invariants

The strategy is recorded as executable invariants in `plans/invariants.json`:

- `ci.windows_test_timeout_bound`
- `ci.coverage_wave_throttle`
- `ci.coverage_job_separate`
- `ci.build_jobs_bounded`
- `ci.incremental_disabled`
