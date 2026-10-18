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

| Workload Type | Profile | Recommended Mechanism | Rationale |
|---|---|---|---|
| **Short Bounded CPU** | JSON parse, small transforms (<100us) | Direct Tokio worker thread | Do NOT use `spawn_blocking`. Overhead outweighs execution time. |
| **Sync / Blocking I/O** | `std::fs`, sync DBs, FFI | `tokio::task::spawn_blocking` | Stalls worker thread if run directly. |
| **Sustained Heavy CPU** | Crypto, compression (>1ms) | `spawn_blocking`, Rayon, or CPU pool | Sustained CPU starves worker scheduler. |
| **Latency Network I/O** | API fan-out, proxying | Async futures + `Semaphore` / backpressure | Use `tokio::sync::Semaphore` to bound fan-out. |

## PR Review Flag Checklist
- [ ] **Cargo-cult `spawn_blocking`**: No offload for short bounded CPU tasks (<100us).
- [ ] **Unbounded Spawn Fan-out**: Loops bounded by `Semaphore` or `backpressure`.
- [ ] **Lock-Across-`.await`**: Drop MutexGuard before `.await` (Lock-Across rule).
- [ ] **Repeatedly-Ready**: No spin loops.
