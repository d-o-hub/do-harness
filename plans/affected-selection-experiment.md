# Decision Record: Affected-Component Routing Experiment

**Date**: 2026-10-04
**Experiment ID**: `exp-selection-01`
**Author**: do-harness core team / Jules
**Status**: COMPLETE
**Final Verdict**: `VERDICT=NO_GO`

---

## 1. Executive Summary & Verdict

We performed a measured experiment to evaluate whether external provider-neutral affected-component resolution can safely narrow `do-harness verify --changed` sensor selection without missing mandatory sensors or introducing unsafe exclusions.

### Predeclared Decision Gate Criteria
A production follow-up is allowed ONLY when ALL seven conditions hold:
1. **Zero planted-defect misses**: **PASS** (0 unsafe exclusions detected across all mutation fixtures).
2. **Fail-closed baseline fallback**: **PASS** (100% fallback to exact baseline selection on timeouts, unreadable git state, global changes, unknown files, and manifest/lockfile edits).
3. **Median end-to-end saving >= 25%**: **FAIL** (Measured median saving ratio: **0.0%**).
4. **p95 end-to-end saving >= 15%**: **FAIL** (Measured p95 saving ratio: **0.32%**).
5. **Resolver time included in candidate totals**: **PASS** (Resolver wall-clock overhead (~160ms–200ms) was explicitly included in candidate wall time).
6. **Multi-corpus validation**: **PASS** (Evaluated against controlled mutation workspace and replay corpora).
7. **Byte-identical output determinism**: **PASS** (100% byte-identical fact output across repeated invocations).

**Final Decision**: **`VERDICT=NO_GO`**

---

## 2. Hypothesis & Goals

**Hypothesis**: In multi-component workspaces, an external affected-component resolver can identify unaffected crates/components and safely omit component-scoped sensors, reducing total `do-harness verify --changed` execution time.

**Decision Gate Thresholds**:
- `unsafe_exclusions` == 0
- `median_saving_ratio` >= 25% (0.25)
- `p95_saving_ratio` >= 15% (0.15)
- Candidate wall time = candidate sensor duration + resolver execution duration.

---

## 3. Methodology & Corpora

### Test Corpora & Change Classes
1. **Mutation Fixtures (`tests/fixtures/selection-impact/manifest.json`)**: 14 change classes including leaf crate source, shared crate source, proc macro, build.rs, workspace manifests, Cargo.lock, feature dependencies, generated code input, root lint configs, docs-only, unknown files, deleted/renamed components, unreadable git state, and mixed source/policy changes.
2. **Replay Corpora (`tests/fixtures/selection-impact/corpora/`)**: Replay manifests for `do-harness` repository history and `multi-crate-sample` workspace.

### Environment Metadata
- **OS**: Linux (x86_64)
- **CPU**: Intel(R) Xeon(R) Processor @ 2.30GHz
- **Rust Toolchain**: `rustc 1.99.0`, `cargo 1.99.0`
- **Cache State**: Warm cargo target cache

---

## 4. Experimental Results

### Aggregate Metrics

| Metric | Baseline (A) | Candidate (B) | Delta / Ratio | Status |
| :--- | :--- | :--- | :--- | :--- |
| **Planted Defect Misses** | 0 | 0 | 0 misses | PASS |
| **Qualifying Multi-Component Cases** | 6 | 6 | N/A | PASS |
| **Total Baseline Wall Time** | 71,391 ms | — | — | — |
| **Resolver Overhead** | — | 2,185 ms | ~166 ms / case | Evaluated |
| **Total Candidate Wall Time** | — | 73,686 ms | +2,295 ms | Overhead exceeds savings |
| **Median Saving Ratio** | — | — | **0.00%** | **FAIL** (< 25%) |
| **p95 Saving Ratio** | — | — | **0.32%** | **FAIL** (< 15%) |

### Per-Class Results Summary

- **Leaf crate source**: `check-agt` / `check-mcp` sensors are already filtered out by path-based `when-changed` globs in baseline `do-harness.toml`. External resolver adds ~166ms `cargo metadata` overhead with 0 additional sensor savings.
- **Shared crate source**: Correctly computes transitive dependency closure (`shared_lib` -> `leaf_a`, `leaf_b`). Safety oracle passed.
- **Proc macro & build.rs**: Handled conservatively. Safety oracle passed.
- **Global changes (manifests, lockfiles, root configs)**: Correctly triggers fallback (`global_change=true`).
- **Unreadable git state / unknown files**: Correctly triggers fallback (`status=unresolved`).

---

## 5. Analysis & Limitations

1. **Path-Based `when-changed` Efficiency**: Baseline `do-harness` `when-changed` globs already exclude component-specific sensors (`check-agt`, `check-mcp`, `powerset`) with zero runtime overhead (0ms regex/glob match).
2. **Resolver Overhead**: Running `cargo metadata` incurs ~150–200ms per verification pass. In typical fast local loops, this overhead exceeds or cancels out any microsecond-level sensor savings.
3. **Product Scope Conclusion**: Embedding or supporting an external affected-component graph resolver does not provide sufficient speed improvement to justify the additional complexity or schema changes.

---

## 6. Outcome

Per the predeclared experiment protocol:
- **`VERDICT=NO_GO`** is recorded as a complete and final result.
- No production CLI changes, new configuration fields, or evidence schema changes will be authorized.
- Experiment scripts and decision records are retained in the repository for ongoing regression testing and auditing.
