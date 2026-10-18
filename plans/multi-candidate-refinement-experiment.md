# Decision Record: Multi-Candidate Skill Refinement Feasibility Experiment

**Date**: 2026-10-18
**Experiment ID**: `exp-skill-refinement-01`
**Author**: do-harness core team
**Status**: COMPLETE
**Final Verdict**: `VERDICT=DEFER`

---

## 1. Executive Summary & Verdict

We evaluated the feasibility and efficacy of evidence-driven multi-candidate skill refinement (inspired by Harness²'s Propose–Probe–Compose loop) within `do-harness`.

### Key Findings
1. **Deterministic Fixtures Enforce Strict Contract Integrity**:
   Deterministic evaluations in `do-harness` check exact structural assertions (`contains:`, `exists:`, `absent:`). When guidance variants omit specific prose patterns or mandatory structure markers, pass rates drop precipitously to 0.00% (as demonstrated by concise/checklist variants of `tokio-performance`).
2. **Concise Guidance Yields Significant Efficiency Gains When Contracts Are Preserved**:
   Where contract terms were fully retained (`htn-planner` candidate 1, `skill-creator` candidate 1), compressing guidance reduced context payload size by **35% to 64%** while preserving a **100% pass rate** and identical deterministic execution runtime (~0.68s and ~1.54s).
3. **Execution Cost vs. Gain Boundary**:
   Probing multiple variants against identical task suites multiplies evaluation time by $N$ (where $N$ is candidate count). In deterministic walkthrough mode, execution is fast (<1s per run); however, under LLM agent execution mode (`eval --agent-cmd`), generating and probing 3 candidate variants per task across 5 iterations would incur significant token/time costs without guaranteeing an improvement over single-trace heuristic distillation (`do-harness distill`).

### Decision Gate Criteria Evaluation
- **Contract & Pass Rate Safety**: **MIXED / FAIL** (Concise & Checklist variants for `tokio-performance` broke assertion expectations, dropping pass rate to 0.00%).
- **Word Count Efficiency Ratio**: **PASS** (Successful concise candidates improved Efficiency Ratio from 3.45 -> 9.71 for `htn-planner` and 0.81 -> 1.26 for `skill-creator`).
- **Runtime Overhead**: **PASS** (Walkthrough execution time was unaffected by guidance length: ~0.68s for `htn-planner`, ~1.55s for `skill-creator`).
- **Run Determinism**: **PASS** (Zero variance in pass rates across 5 repeated iterations; `pass_rate_variance = 0.0`).

**Final Decision**: **`VERDICT=DEFER`**

---

## 2. Hypothesis & Goals

**Hypothesis**: Automatically generating and probing multiple guidance candidates ($N \ge 3$) against an existing skill evaluation suite will identify higher-efficiency variants that preserve or increase Skill Lift while reducing context word count.

**Evaluation Framework**:
- **Baseline (A)**: Existing canonical `.agents/skills/<skill>/SKILL.md`.
- **Candidates (B1, B2, B3)**:
  - `candidate_1_concise`: Streamlined guidance focusing strictly on rules (~35–64% word count reduction).
  - `candidate_2_verbose`: Expanded guidance with narrative context and background (+30–50% word count).
  - `candidate_3_checklist`: Formatted as strict tabular/bulleted validation checklists.
- **Fixed Conditions**: Model, executor, prompts, timeouts, and eval assertions held constant across 5 iterations.

---

## 3. Experimental Methodology & Corpus

### Selected Skills & Case Inventory
1. `htn-planner`: 3 cases / 9 assertions (Explicit, Implicit, Negative). Evaluates HTN decomposition, method catalog, preconditions.
2. `tokio-performance`: 4 cases / 14 assertions (Explicit, Implicit, Contextual, Negative). Evaluates async offloading and review flags.
3. `skill-creator`: 3 cases / 8 assertions (Explicit, Implicit, Negative). Evaluates skill scaffolding and structure gating.

### Environment Metadata
- **OS**: Linux (x86_64)
- **Rust Toolchain**: `rustc 1.99.0`
- **Harness Build**: `do-harness 0.3.0` (debug target)
- **Iterations**: 5 repeated benchmark runs per variant

---

## 4. Experimental Results

### Aggregate Metrics Summary

| Skill | Variant | Words | Avg Pass Rate | Pass Rate Var | Avg Lift | Avg Walk (s) | Efficiency Ratio |
|---|---|---|---|---|---|---|---|
| **htn-planner** | baseline | 290 | **1.0000** | 0.0000 | +0.4444 | 0.6852s | 3.4483 |
| | candidate_1_concise | 103 | **1.0000** | 0.0000 | +0.4444 | 0.6840s | **9.7087** |
| | candidate_2_verbose | 387 | **1.0000** | 0.0000 | +0.4444 | 0.6899s | 2.5840 |
| | candidate_3_checklist | 144 | **1.0000** | 0.0000 | +0.4444 | 0.6847s | 6.9444 |
| **tokio-performance** | baseline | 495 | **1.0000** | 0.0000 | +1.0000 | 0.2029s | 2.0202 |
| | candidate_1_concise | 149 | **0.0000** | 0.0000 | 0.0000 | 0.0076s | 0.0000 |
| | candidate_2_verbose | 333 | 0.9286 | 0.0000 | +0.9286 | 0.2036s | 2.7885 |
| | candidate_3_checklist | 209 | **0.0000** | 0.0000 | 0.0000 | 0.0077s | 0.0000 |
| **skill-creator** | baseline | 1237 | **1.0000** | 0.0000 | +1.0000 | 1.5601s | 0.8084 |
| | candidate_1_concise | 796 | **1.0000** | 0.0000 | +1.0000 | 1.5434s | **1.2563** |
| | candidate_2_verbose | 1007 | **1.0000** | 0.0000 | +1.0000 | 1.5419s | 0.9930 |
| | candidate_3_checklist | 826 | **1.0000** | 0.0000 | +1.0000 | 1.6550s | 1.2107 |

---

## 5. Critical Analysis & Lessons

1. **Why Candidate Probing Proved Risk-Prone**:
   In `tokio-performance`, the evaluation suite contained assertions verifying that exact phrases in `SKILL.md` were present (e.g., `contains:.agents/skills/tokio-performance/SKILL.md|Semaphore`). Refactoring variants to omit or rephrase these terms caused complete assertion failure (pass rate 0.00%).
2. **Concise Refinement Potential**:
   When rephrasing maintains exact contractual terms, concise variants achieved a **2.8x efficiency increase** (`htn-planner`) and **1.5x efficiency increase** (`skill-creator`) without dropping a single point of pass rate or Skill Lift.
3. **Comparison with Existing `do-harness` Mechanisms**:
   - `do-harness distill`: Distills proven heuristics from verified traces into `references/heuristics.md` deterministically based on real sensor beats.
   - `do-harness learn`: Incorporates recurring sensor failure signatures without candidate generation overhead.
   - Multi-candidate refinement (Propose–Probe–Compose) adds significant execution complexity ($N \times$ evaluation runs) which is only justified if guided by an automated contract-preserving parser.

---

## 6. Recommendations & Next Steps

- **Recommendation**: **DEFER** adding an automated multi-candidate orchestration loop or CLI subcommand at this time.
- **Rationals**:
  1. Existing single-trace distillation (`do-harness distill`) and sensor-driven learning (`do-harness learn`) provide predictable, deterministic improvements without candidate search overhead.
  2. The risk of candidate variant generation dropping contractual keywords outweighs the marginal context savings unless candidates are validated against a formal syntax tree.
- **Future Work (If revisited)**:
  - Implement static contract checking for candidates before probing to prevent keyword drop regressions.
  - If requested by users, any future multi-candidate refinement tool should be an opt-in offline script under `scripts/` rather than a core binary runtime feature.
