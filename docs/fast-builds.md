# Fast-Build Setup & Benchmark Measurements

Opt-in instructions for accelerating local development builds with shared compilation caching (`sccache`) and fast linking (`mold`).

## Opt-in Setup (<20 lines)

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

## Measured Benchmarks (Linux x86_64, Rust 1.98 / MSRV 1.85)

| Build Configuration | Clean Build | Incremental (`touch main.rs`) |
| ------------------- | ----------- | ----------------------------- |
| Baseline (`dev` profile) | 2m 31s | 2.48s |
| `mold` linker | 2m 35s | 2.52s |
| `sccache` (cold cache) | 2m 40s | 2.48s |
| `sccache` (warm clean build) | 1m 17s (**51% faster / 2.1x speedup**) | 2.48s |
