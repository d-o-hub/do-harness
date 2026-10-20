# Fast-Build Setup & Benchmark Measurements

Opt-in instructions for accelerating local development builds with shared compilation caching (`sccache`) and fast linking (`mold`).

## Opt-in Setup (<20 lines)

<!-- example: id=fast_builds_opt_in classification=illustrative reason="Opt-in environment setup instructions" -->
```bash
# 1. Install tools (e.g. Linux x86_64)
sudo apt-get install sccache mold clang

# 2. Enable sccache compilation cache (local or team S3/GCS backend)
export RUSTC_WRAPPER=sccache
# Team remote caching pointers: set SCCACHE_BUCKET / SCCACHE_GCS_BUCKET environment variables.

# 3. Enable mold linker (Linux)
export RUSTFLAGS="-C link-arg=-fuse-ld=mold"
# Alternatively, uncomment [target.x86_64-unknown-linux-gnu] in .cargo/config.toml
```

For CI builds where full release optimization is overkill, pass `--profile ci` (`lto = "thin"`, `codegen-units = 16`).

## Low-memory builds

Cargo otherwise starts up to one compiler process per logical CPU. That is a
good throughput default, but it can create a large aggregate RSS when several
large dependencies compile together. Use a bounded job count on a constrained
machine rather than changing the repository-wide default:

<!-- example: id=fast_builds_low_mem classification=illustrative reason="Low memory cargo build commands" -->
```bash
# Lowest peak memory; slowest clean build.
CARGO_BUILD_JOBS=1 cargo check --workspace --locked

# Balanced constrained-machine setting.
CARGO_BUILD_JOBS=2 cargo check --workspace --locked
CARGO_BUILD_JOBS=2 cargo build --workspace --locked
```

CI sets `CARGO_BUILD_JOBS=2` and `CARGO_INCREMENTAL=0` in the build workflows.
The job bound applies to Cargo's compiler-process scheduling; the incremental
setting avoids retaining CI-only incremental state. The two instrumented gates
(`release` preflight and scheduled `coverage`) go one step further and run with
`CARGO_BUILD_JOBS=1` and `NEXTEST_TEST_THREADS=1`: `cargo llvm-cov nextest` is
the peak-RSS path in this repository, and a hosted runner starved under memory
pressure is shut down rather than reporting a sensor failure. Those caps bound
the damage; they do not remove it. A test that spawns the harness, `init`
included, must clear the outer coverage session with
`support::isolate_command`, or its own cargo build inherits `RUSTC_WRAPPER` and
`CARGO_TARGET_DIR` and runs a second instrumented workspace: the `concurrency`
fixture did exactly that and pushed the `release`/`scheduled` gates past 15 GB
with no single process above ~600 MB RSS. `cargo check` is a fast feedback
command, but a release build is still required to validate final code generation
and linking.

On this workspace, a clean, non-incremental `cargo check --workspace` measured
approximately 2.05 GiB peak sampled compiler-process RSS with eight jobs and
0.82 GiB with two jobs. The two-job run took longer, so the bounded setting is
applied to CI for memory headroom and is left opt-in for local development.

The `codegen-units = 1`, LTO, and linker alternatives remain profile- or
platform-specific experiments. They are not enabled as universal memory fixes:
they can trade peak memory for longer compilation or different generated code.

## Measured Benchmarks (Linux x86_64, Rust 1.98 / MSRV 1.85)

| Build Configuration | Clean Build | Incremental (`touch main.rs`) |
| ------------------- | ----------- | ----------------------------- |
| Baseline (`dev` profile) | 2m 31s | 2.48s |
| `mold` linker | 2m 35s | 2.52s |
| `sccache` (cold cache) | 2m 40s | 2.48s |
| `sccache` (warm clean build) | 1m 17s (**51% faster / 2.1x speedup**) | 2.48s |
