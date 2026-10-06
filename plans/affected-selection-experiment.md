# Decision Record: Affected-Component Routing Experiment

**Date**: 2026-10-05
**Experiment ID**: `exp-selection-01`
**Author**: do-harness core team
**Status**: COMPLETE
**Final Verdict**: `VERDICT=NO_GO`

---

## 1. Executive Summary & Verdict

We performed a measured experiment to evaluate whether external provider-neutral affected-component resolution can safely narrow `do-harness verify --changed` sensor selection without missing mandatory sensors or introducing unsafe exclusions.

### Predeclared Decision Gate Criteria
A production follow-up is allowed ONLY when ALL seven conditions hold:
1. **Zero planted-defect misses**: **PASS** (0 unsafe exclusions across all 14 change classes and mutation fixtures; mutation-based safety oracle proven for all planted defects).
2. **Fail-closed baseline fallback**: **PASS** (100% fallback to exact baseline selection on unreadable git state, global changes, unknown files, manifest/lockfile edits, and resolver errors).
3. **Median end-to-end saving >= 25%**: **FAIL** (While isolated synthetic sub-cases with long-running sensors showed narrow savings, overall workspace wall time increased from 25,628 ms to 26,971 ms due to resolver execution overhead).
4. **p95 end-to-end saving >= 15%**: **FAIL** (In realistic multi-component repositories like `do-harness` and `multi-crate-sample`, Rust compilation and test sensors are workspace-wide, yielding 0.0% sensor reduction and pure negative overhead).
5. **Resolver time included in candidate totals**: **PASS** (Resolver wall-clock overhead (~558 ms per invocation) was explicitly included in candidate wall time).
6. **Multi-corpus validation**: **PASS** (Evaluated against controlled mutation workspace `tests/fixtures/selection-impact/workspace` and replay corpora `corpus-do-harness-history.json` and `corpus-multi-crate-sample.json`).
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
- **CPU**: Intel(R) Core(TM) i5-8350U CPU @ 1.70GHz
- **Rust Toolchain**: `rustc 1.99.0`, `cargo 1.99.0`
- **Cache State**: Warm cargo target cache

---

## 4. Experimental Results

### Aggregate Metrics

| Metric | Baseline (A) | Candidate (B) | Delta / Ratio | Status |
| :--- | :--- | :--- | :--- | :--- |
| **Planted Defect Misses** | 0 | 0 | 0 misses | PASS (Oracle proven) |
| **Qualifying Multi-Component Cases** | 6 | 6 | N/A | PASS |
| **Total Baseline Wall Time** | 25,628 ms | — | — | Baseline |
| **Resolver Overhead** | — | 7,818 ms | ~558 ms / case | Evaluated |
| **Candidate Sensor Wall Time**| — | 19,153 ms | −6,475 ms | Sensor savings |
| **Total Candidate Wall Time** | — | 26,971 ms | **+1,343 ms (+5.2%)** | **Overhead exceeds savings** |
| **Median Saving Ratio** | — | — | 30.2% (isolated) | Mixed |
| **Overall Net Saving** | — | — | **−5.2%** | **FAIL** (Net slower) |

### Per-Class Results Summary

- **Leaf crate source**: Correctly narrows to component sensors. However, resolver `cargo metadata` overhead (~534 ms) offsets sensor execution savings.
- **Shared crate source**: Correctly computes transitive dependency closure (`shared_lib` -> `leaf_a`, `leaf_b`). Safety oracle passed: planted defect in `shared_lib` was demonstrably caught by downstream `test-leaf-a`.
- **Proc macro & build.rs**: Handled conservatively. Safety oracle passed.
- **Global changes (manifests, lockfiles, root configs)**: Correctly triggers fallback (`global_change=true`).
- **Unreadable git state / unknown files**: Correctly triggers fallback (`status=unresolved`).

---

## 5. Analysis & Limitations

1. **Path-Based `when-changed` Efficiency**: Baseline `do-harness` `when-changed` globs already exclude component-specific sensors (`check-agt`, `check-mcp`, `powerset`) with zero runtime overhead (0 ms regex/glob match).
2. **Resolver Overhead**: Running `cargo metadata` incurs ~350–650 ms per invocation. In typical fast local loops, this overhead exceeds or cancels out any microsecond-level sensor savings, resulting in a net negative performance impact across realistic verification suites.
3. **Product Scope Conclusion**: Embedding or supporting an external affected-component graph resolver does not provide sufficient speed improvement to justify the additional complexity, protocol maintenance, or schema changes.

---

## 6. Outcome

Per the predeclared experiment protocol:
- **`VERDICT=NO_GO`** is recorded as a complete and final result.
- No production CLI changes, new configuration fields, or evidence schema changes will be authorized.
- Experiment scripts, fixtures, and decision records are retained in the repository for regression testing and ongoing auditing.
