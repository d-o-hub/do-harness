---
name: tokio-performance
description: >
  Workload-aware Tokio performance rules, task offloading strategy, and async
  code review flags. Use when writing async Rust code, choosing between worker
  execution vs spawn_blocking/pools, or reviewing async PRs for latency or
  blocking bugs. Triggers: "tokio", "async", "spawn_blocking", "performance",
  "concurrency", "lock-across-await".
license: MIT
metadata:
  version: "0.1.0"
  tags: tokio async performance concurrency spawn_blocking workload-aware
---

# Workload-Aware Tokio Guidance

## Task Offloading Rules
- **Short Bounded CPU** (<100us, e.g. JSON parse): Direct Tokio worker thread. Do NOT offload to `spawn_blocking`.
- **Sync I/O**: `tokio::task::spawn_blocking`.
- **Sustained / Heavy CPU** (>1ms, e.g. crypto, compression): Use `spawn_blocking`, Rayon, or CPU pool.
- **Latency Network I/O**: Async futures + `tokio::sync::Semaphore` or backpressure channels.

## Review Flags
1. **Cargo-cult `spawn_blocking`**: Keep short CPU tasks on worker thread.
2. **Unbounded Spawn Fan-out**: Bound loops with `Semaphore` or `backpressure`.
3. **Lock-Across-`.await`**: Drop MutexGuard before `.await` (Lock-Across rule).
4. **Repeatedly-Ready**: No spin loops.
