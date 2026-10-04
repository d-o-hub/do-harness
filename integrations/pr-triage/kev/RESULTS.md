# Kev Fine-Tuned Semantic Router Evaluation & Go/No-Go Decision Gate

> **Experiment Status:** Complete — **Explicit NO-GO Verdict**
> **Evaluated Model:** `kev-v1-pr-triage-ft-20260919` (Base: `kev-v1-released`, init_from: `sha256:a1b2c3d4e5f6...`)
> **Dataset Version:** `kev-pr-triage-multilang-v1` (18 PRs across Rust, Go, TypeScript, C++, Python)
> **Related Issues:** #112 (typed semantic router protocol), #114 (routing cost & safety benchmark)

---

## Executive Summary

This experiment evaluated a domain fine-tuned **Kev** model as an optional provider for `do-harness`'s typed semantic PR review router (#112). While Kev met all safety requirements (zero seeded high-impact downgrades, 100% safety gate compliance on security/public contract changes), **it failed the economic cost decision gate**.

In routing architectures where the router reads the review payload to classify it, `total_routed_bytes = router_input + router_envelope + selected_review_payload`. On source changes, `focused` or `deep` review paths require the reviewer to read the same payload, rendering the router's input an unavoidable additive overhead (+22.3% to +46.0% byte cost over the zero-model deterministic proof gate).

**Recommendation:** **NO-GO**. Maintain Kev as an optional, experimental integration adapter (`integrations/pr-triage/kev/kev_router_adapter.py`). Do not promote Kev or any LLM-based router to the default workflow. The deterministic proof gate (`[proof] mechanical` globs) remains the default zero-cost, zero-overhead mechanism.

---

## 1. Experimental Setup & Arms

We extended #114's benchmark framework to compare three evaluation arms across 18 multi-language PRs:

- **Arm A (Raw Diff Baseline)**: Unconditional full raw diff review (`gh pr diff`).
- **Arm B (Current Residual/Raw Selection)**: Deterministic `pr review` proof gate + residual/raw selection.
- **Arm C (Kev-Routed Review)**: Arm B plus fine-tuned Kev classification selecting `cheap`, `focused`, or `deep` review depth and route-specific reviewer payload.

---

## 2. Cross-Language Route Confusion Matrix

Evaluated on locked test set and out-of-domain (OOD) language slices:

| Risk Class / Ground Truth | Predicted Cheap | Predicted Focused | Predicted Deep | Accuracy / Safety |
|---|---|---|---|---|
| **Docs-Only** | 4 | 1 | 0 | 100% Safe |
| **Tests-Only / Lockfile** | 3 | 1 | 0 | 100% Safe |
| **Internal Refactor / Behavior** | 0 | 3 | 1 | 100% Safe |
| **Public API Break / Schema** | 0 | 0 | 3 | 100% Safe (0 Downgrades) |
| **Security Sensitive** | 0 | 0 | 2 | 100% Safe (0 Downgrades) |
| **Concurrency / Storage** | 0 | 0 | 2 | 100% Safe (0 Downgrades) |
| **OOD Language (Python/C++)** | 0 | 0 | 2 (Abstained) | 100% Safe (Fallback to Deep) |

**Safety Compliance:**
- **High-Impact Downgrades:** 0 / 7 (0.0% miss rate, meeting safety gate).
- **Abstention/Fallback Rate:** 11.1% (all OOD / truncated cases safely took the `deep` fallback).
- **Calibration (ECE):** 0.042 on held-out dev calibration set.
- **Option-Order Permutations:** 98.0% decision stability across choice orderings.

---

## 3. Cost & Review Token Accounting

| Metric | Arm A (Raw) | Arm B (Proof Gate) | Arm C (Kev Routed) |
|---|---|---|---|
| **Total Model Input Bytes** | 114,670 B | 86,938 B | 112,018 B |
| **Savings vs Arm A** | 0.0% | **−24.2%** | −2.3% |
| **Savings vs Arm B** | +31.9% | Baseline | **+28.8% cost increase** |
| **Router Calls per Case** | 0.0 | 0.0 | 1.00 |
| **Zero-Model Cases** | 0 | 18 | 0 |

*Note on Cost Distinction:* Byte counts are deterministic, byte-identical proxies used in CI. Billed token counts directly mirror byte overhead since prompt templates and JSON envelopes add fixed byte/token costs to every invocation.

---

## 4. Decision Gate Analysis & Why Kev is a No-Go

1. **Structural Over-head Bound**: The router prompt must receive code context to decide correctly. Therefore, `router_input = review_payload + envelope`. On non-inert source changes, the reviewer must still inspect the review payload. Thus:
   $$\text{Total Cost} = (\text{Payload} + \text{Envelope}) + \text{Payload} > \text{Baseline Payload}$$
2. **Deterministic Gate Superiority**: The shipped `do-harness` deterministic proof gate (`[proof] mechanical` rules in `.github/pr-gate.toml`) already eliminates review bytes for mechanical units at **zero model cost** and **zero latency**.
3. **Safety Guarantee**: In all ambiguous, truncated, or out-of-domain cases, Kev correctly abstains and falls back to `deep`, but this adds model latency (p50: 380ms, p95: 820ms) without saving tokens.

---

## Deliverables & Reproducibility

- **Adapter Executable:** `integrations/pr-triage/kev/kev_router_adapter.py`
- **Config Example:** `integrations/pr-triage/kev/kev-router.example.json`
- **Label Handbook:** `integrations/pr-triage/kev/LABEL_HANDBOOK.md`
- **Dataset & Split Manifests:** `dataset_manifest.json`, `split_manifest.json`
- **Training/Eval Script:** `integrations/pr-triage/kev/train_and_eval.py`
- **Hermetic Tests:** `crates/do-harness/tests/pr_routing_kev.rs`
