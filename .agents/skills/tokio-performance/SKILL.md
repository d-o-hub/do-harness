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

## Overview
Async execution must be workload-aware: `spawn_blocking` is not a magic speedup. Offloading short, bounded CPU work to a blocking thread pool incurs thread context-switch and synchronization overhead that often exceeds the execution time of the work itself.

This skill mirrors the upstream fix in template #336 (`rust-2026-template:.agents/skills/tokio-performance/SKILL.md`).

---

## Decision Table: Task Offloading Strategy

| Workload Type | Execution Profile | Recommended Mechanism | Rationale & Anti-patterns |
|---|---|---|---|
| **Short Bounded CPU** | In-memory JSON parsing, small data transformations (<10–100 µs) | Execute directly on Tokio worker thread | **Do NOT** offload to `spawn_blocking`. Thread dispatch overhead outweighs execution time. |
| **Genuinely Blocking / Synchronous I/O** | `std::fs` file operations, synchronous DB drivers, blocking FFI | `tokio::task::spawn_blocking` or dedicated OS thread/pool | Stalls worker thread if run directly. Offload keeps Tokio executor responsive. |
| **Sustained / Heavy CPU** | Heavy cryptography, compression, large-scale processing (>1 ms) | `spawn_blocking`, Rayon, or bounded thread pool | Prevents worker thread starvation across multi-tenant or concurrent operations. |
| **Latency-Sensitive Network I/O** | Proxy forwarding, HTTP/gRPC requests, stream handling | Non-blocking async futures on worker threads with bounded concurrency | Use `tokio::sync::Semaphore` or backpressure channels for fan-out. |

---

## Async Code Review Flags

When writing or reviewing async code, look for these specific anti-patterns:

1. **Cargo-cult `spawn_blocking`**:
   - *Violation*: Wrapping short, bounded CPU operations (e.g., small JSON deserialization, string format) in `spawn_blocking`.
   - *Fix*: Keep short bounded CPU operations inline on the worker thread.

2. **Unbounded Spawn Fan-out**:
   - *Violation*: Spawning tasks (`tokio::spawn`) inside an unbounded loop without semaphore or channel backpressure.
   - *Fix*: Limit concurrency using `tokio::sync::Semaphore` or bounded channels (e.g. `mpsc::channel(buffer)`).

3. **Lock-Across-`.await`**:
   - *Violation*: Holding a synchronous `std::sync::MutexGuard` or `RefCell` across an `.await` boundary.
   - *Fix*: Drop the guard before awaiting, narrow lock scope, or use `tokio::sync::Mutex` if holding across yields is unavoidable.

4. **Repeatedly-Ready / Spin Loops**:
   - *Violation*: Tight `tokio::select!` or yield loops without cancellation checks or backoff that consume 100% CPU.
   - *Fix*: Ensure futures yield properly or incorporate timer/backoff mechanisms.

---

## Existing Codebase Async Review

As part of adopting this guidance (template #336 mirror), existing async offloads in the repository were evaluated:

- **`crates/guardian-proxy/src/server.rs` (`record_audit`)**: Offloads synchronous `std::fs` file append with mutex locking via `tokio::task::spawn_blocking`.
  - *Verdict*: **Already Fine**. Legitimate synchronous disk I/O offload to prevent stalling the single-threaded/current-thread worker runtime.
- **`crates/do-harness/src/eval/` (`agent.rs`, `grading.rs`)**: Uses `spawn_blocking` for running sub-processes and walkthrough scripts.
  - *Verdict*: **Already Fine**. Legitimate blocking process execution during evaluation runs.

---

## References
- Upstream template issue: `rust-2026-template` closed issue #336.
- Tokio Documentation: [Cpu-bound tasks and blocking code](https://tokio.rs/tokio/tutorial/spawning#cpu-bound-tasks).
