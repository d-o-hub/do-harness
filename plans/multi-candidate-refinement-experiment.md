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
1. **Walkthrough & Structure Contracts Must Be Formally Preserved**:
   In deterministic evaluations, walkthrough scripts (`walkthrough.sh`) may extract specific headings (e.g. `## Decision Table`). Candidate generation must formally preserve these structural contracts. When headings were properly preserved, all candidate variants across all three target skills achieved **100.00% pass rates** across 5 repeated iterations.
2. **Concise Guidance Yields Consistent Efficiency Gains**:
   Across all tested skills (`htn-planner`, `tokio-performance`, `skill-creator`), concise variants (`candidate_1_concise`) reduced context size by **35% to 66%** while maintaining identical **100.00% pass rates** and +1.00 Skill Lift. Efficiency ratio (`pass_rate / words * 1000`) increased by:
   - `htn-planner`: **3.45 -> 9.71** (+181% efficiency)
   - `tokio-performance`: **2.02 -> 5.95** (+194% efficiency)
   - `skill-creator`: **0.81 -> 1.26** (+55% efficiency)
3. **Execution Cost vs. Gain Boundary**:
   While deterministic walkthrough execution is fast (~0.22s to 1.78s per run), evaluating multiple candidates ($N \ge 3$) under live LLM agent execution (`eval --agent-cmd`) multiplies API costs and wall-clock latency by $N \times \text{iterations}$.

### Decision Gate Criteria Evaluation
- **Contract & Pass Rate Safety**: **PASS** (100.00% pass rate maintained across all 12 candidate combinations once structural contracts were preserved).
- **Word Count Efficiency Ratio**: **PASS** (Concise variants yielded up to +194% efficiency gain per context byte).
- **Runtime Overhead**: **PASS** (Deterministic walkthrough execution time was unaffected by guidance length: ~0.78s for `htn-planner`, ~0.22s for `tokio-performance`, ~1.75s for `skill-creator`).
- **Run Determinism**: **PASS** (Zero variance in pass rates across 5 repeated iterations; `pass_rate_variance = 0.0`).

**Final Decision**: **`VERDICT=DEFER`**

---

## 2. Hypothesis & Goals

**Hypothesis**: Automatically generating and probing multiple guidance candidates ($N \ge 3$) against an existing skill evaluation suite will identify higher-efficiency variants that preserve or increase Skill Lift while reducing context word count.

**Evaluation Framework**:
- **Baseline (A)**: Existing canonical `.agents/skills/<skill>/SKILL.md`.
- **Candidates (B1, B2, B3)**:
  - `candidate_1_concise`: Streamlined guidance focusing strictly on rules (~35–66% word count reduction).
  - `candidate_2_verbose`: Expanded narrative guidance.
  - `candidate_3_checklist`: Formatted as strict tabular/bulleted validation checklists.
- **Fixed Conditions**: Model, executor, prompts, timeouts, and eval assertions held constant across 5 iterations.

---

## 3. Experimental Methodology & Corpus

### Selected Skills & Case Inventory
1. `htn-planner`: 3 cases / 9 assertions (Explicit, Implicit, Negative). Evaluates HTN decomposition, method catalog, preconditions.
2. `tokio-performance`: 4 cases / 14 assertions (Explicit, Implicit, Contextual, Negative). Evaluates async offloading and review flags.
3. `skill-creator`: 3 cases / 8 assertions (Explicit, Implicit, Negative). Evaluates skill scaffolding and structure gating.

### Environment & Provenance Metadata
- **OS**: Linux (x86_64)
- **Rust Toolchain**: `rustc 1.99.0`
- **Harness Build**: `do-harness 0.3.0` (debug target)
- **Evaluator Mode**: Deterministic (`eval --format json`)
- **Iterations**: 5 repeated benchmark runs per variant (60 total evaluated runs)

---

## 4. Experimental Results

### Aggregate Metrics Summary

| Skill | Variant | Words | Avg Pass Rate | Pass Rate Var | Avg Lift | Avg Walk (s) | Efficiency Ratio |
|---|---|---|---|---|---|---|---|
| **htn-planner** | baseline | 290 | **1.0000** | 0.0000 | +0.4444 | 0.7926s | 3.4483 |
| | candidate_1_concise | 103 | **1.0000** | 0.0000 | +0.4444 | 0.7845s | **9.7087** |
| | candidate_2_verbose | 387 | **1.0000** | 0.0000 | +0.4444 | 0.7794s | 2.5840 |
| | candidate_3_checklist | 144 | **1.0000** | 0.0000 | +0.4444 | 0.7782s | 6.9444 |
| **tokio-performance** | baseline | 495 | **1.0000** | 0.0000 | +1.0000 | 0.2203s | 2.0202 |
| | candidate_1_concise | 168 | **1.0000** | 0.0000 | +1.0000 | 0.2247s | **5.9524** |
| | candidate_2_verbose | 324 | **1.0000** | 0.0000 | +1.0000 | 0.2171s | 3.0864 |
| | candidate_3_checklist | 207 | **1.0000** | 0.0000 | +1.0000 | 0.2295s | 4.8309 |
| **skill-creator** | baseline | 1237 | **1.0000** | 0.0000 | +1.0000 | 1.7393s | 0.8084 |
| | candidate_1_concise | 796 | **1.0000** | 0.0000 | +1.0000 | 1.7876s | **1.2563** |
| | candidate_2_verbose | 1007 | **1.0000** | 0.0000 | +1.0000 | 1.6956s | 0.9930 |
| | candidate_3_checklist | 826 | **1.0000** | 0.0000 | +1.0000 | 1.7610s | 1.2107 |

---

## 5. Critical Analysis & Lessons

1. **Walkthrough & Structural Rigidity**:
   Skill walkthroughs may rely on exact Markdown heading names (e.g. `## Decision Table` extracted by `grep`). Candidate generation must respect these structural contracts.
2. **Concise Refinement Efficacy**:
   When structural contracts are preserved, concise variants achieve up to **2.9x efficiency improvements** without sacrificing pass rates or Skill Lift.
3. **Comparison with Existing `do-harness` Mechanisms**:
   - `do-harness distill`: Distills proven heuristics from verified traces into `references/heuristics.md` deterministically based on real sensor beats.
   - `do-harness learn`: Incorporates recurring sensor failure signatures without candidate generation overhead.
   - Automated candidate refinement (Propose–Probe–Compose) adds runtime complexity ($N \times$ evaluation runs) which is better deferred in favor of existing deterministic mechanisms.

---

## 6. Recommendations & Next Steps

- **Recommendation**: **DEFER** adding an automated multi-candidate orchestration loop or CLI subcommand at this time.
- **Rationales**:
  1. Existing single-trace distillation (`do-harness distill`) and sensor-driven learning (`do-harness learn`) provide predictable, deterministic improvements without candidate search overhead.
  2. Multi-candidate probing introduces $N \times$ execution overhead that is not justified for core binary automation at present.
