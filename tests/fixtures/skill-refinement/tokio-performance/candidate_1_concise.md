---
name: tokio-performance
description: Workload-aware Tokio performance rules and async code review flags.
license: MIT
metadata:
  version: "0.1.0"
---

# Workload-Aware Tokio Guidance

## Decision Table: Task Offloading Strategy

| Workload Type | Profile | Recommended Mechanism | Rationale |
|---|---|---|---|
| **Short Bounded CPU** | JSON parse, small transforms (<100us) | Direct Tokio worker thread | Do NOT use `spawn_blocking`. Overhead outweighs execution time. |
| **Sync / Blocking I/O** | `std::fs`, sync DBs, FFI | `tokio::task::spawn_blocking` | Stalls worker thread if run directly. |
| **Sustained Heavy CPU** | Crypto, compression (>1ms) | `spawn_blocking`, Rayon, or CPU pool | Sustained CPU starves worker scheduler. |
| **Latency Network I/O** | API fan-out, proxying | Async futures + `Semaphore` / backpressure | Use `tokio::sync::Semaphore` to bound fan-out. |

## Async Code Review Flags

1. **Cargo-cult `spawn_blocking`**: No offload for short bounded CPU tasks (<100us).
2. **Unbounded Spawn Fan-out**: Loops bounded by `Semaphore` or `backpressure`.
3. **Lock-Across-`.await`**: Drop MutexGuard before `.await` (Lock-Across rule).
4. **Repeatedly-Ready**: No spin loops.
