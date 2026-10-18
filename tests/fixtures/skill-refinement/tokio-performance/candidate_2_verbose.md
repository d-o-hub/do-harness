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

# Workload-Aware Tokio Guidance & Advanced Execution Architecture

## Overview & Performance Principles
Async Rust execution in Tokio relies on a multi-threaded work-stealing scheduler. Worker threads handle non-blocking asynchronous futures efficiently. Misunderstanding task offloading trade-offs introduces performance degradation, tail latency, and worker starvation.

`spawn_blocking` is not a magic speedup. Offloading short bounded CPU work to the blocking thread pool incurs context switching and synchronization overhead that outweighs the work itself.

## Decision Table: Task Offloading Strategy

| Workload Type | Execution Profile | Recommended Mechanism | Rationale |
|---|---|---|---|
| **Short Bounded CPU** | JSON parse, small transforms (<100 µs) | Direct Tokio worker thread | Do NOT offload to `spawn_blocking`. Overhead dominates. |
| **Genuinely Blocking / Sync I/O** | `std::fs`, sync DB drivers, FFI | `tokio::task::spawn_blocking` or pool | Stalls worker thread if run directly. |
| **Sustained / Heavy CPU** | Crypto, compression, large sorts (>1 ms) | `spawn_blocking`, Rayon, or CPU pool | Prevents worker thread starvation. |
| **Latency Network I/O** | Proxy forwarding, HTTP/gRPC API requests | Async futures + `Semaphore` / backpressure | Use `tokio::sync::Semaphore` or backpressure channels for fan-out. |

## Async Code Review Flags & Checklist

1. **Cargo-cult `spawn_blocking`**:
   - Wrap short bounded CPU operations in `spawn_blocking`? Fix: Keep on worker thread. Note: `spawn_blocking is always faster` is false.
2. **Unbounded Spawn Fan-out**:
   - Calling `tokio::spawn` inside an unbounded loop? Fix: Limit concurrency using `Semaphore` or `backpressure` channels.
3. **Lock-Across-`.await`**:
   - Holding synchronous `std::sync::MutexGuard` across an `.await` boundary? Fix: Drop guard before awaiting or narrow lock scope (Lock-Across rule).
4. **Repeatedly-Ready / Spin Loops**:
   - Tight yield loops consuming 100% CPU? Fix: Incorporate timer or backoff.
