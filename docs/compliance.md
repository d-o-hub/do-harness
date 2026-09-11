# Compliance mapping

> **Assessment date:** 2026-09-11 · **Harness version:** 0.1.0 · **Scope:** dev-loop only
>
> Pinned framework editions: **OWASP Top 10 for Agentic Applications (2026, ASI01–ASI10)**;
> **NIST AI RMF 1.0 (NIST AI 100-1, January 2023)**;
> **EU AI Act, Regulation (EU) 2024/1689**;
> **SOC 2 Trust Services Criteria (2017, revised 2022)**.

## Scope & Positioning

`do-harness` is a **dev-loop verification harness** (feedforward guides + feedback
sensors, workflow gates, tamper-evident logs, and evidence artifacts). It is
**not** a runtime policy engine, inline proxy, or production agent runtime.

Compliance coverage is claimed strictly for **build-time, workflow-level, and
dev-loop verification controls**. In particular:

- The **EU AI Act** rows below are *dev-loop scope only*. `do-harness` is not a
  high-risk AI system and this document is **not** a conformity assessment under
  Regulation (EU) 2024/1689.
- **NIST AI RMF** mappings describe how the harness supports organisational
  GOVERN/MAP/MEASURE/MANAGE practices during development. HTN task
  decomposition orders *engineering work*, not AI-system risk categorization.
- Every `✅` row below links the file or command that enforces it; rows without
  evidence are marked partial.

> **Adjacent optional runtime:** `crates/guardian-proxy` is a separate,
> off-by-default, fail-closed sidecar (requires the `agt-governance` feature).
> It validates upstreams against an SSRF policy, caps request/response bodies,
> times out upstream calls, denies all traffic when mediation is unavailable,
> and can bind decisions to a hash-chained JSONL audit log with cross-process
> file locking (`crates/guardian-proxy/README.md`). It is **not** part of the
> dev-harness compliance boundary and its threat model is documented separately
> in [`docs/threat-model-proxy.md`](threat-model-proxy.md).

---

## High-Level Control Matrix

| do-harness Control | Mechanism | OWASP Agentic Top 10 | NIST AI RMF | EU AI Act (dev-loop) | Evidence |
|---|---|---|---|---|---|
| **Computational sensors** (`verify`) | Deterministic checks strictly supersede LLM self-assessment | ASI04, ASI05, ASI09 | MEASURE 1 | Art. 15 | [`sensors/mod.rs`](../crates/do-harness/src/sensors/mod.rs), [`do-harness.toml`](../do-harness.toml) |
| **Task-completion gate** (`task done`) | Re-checks named sensor beats inside the write transaction | ASI02, ASI08, ASI10 | MANAGE 1 | Art. 14 | [`repo_workflow.rs`](../crates/db/src/repo_workflow.rs), [`task.rs`](../crates/do-harness/src/task.rs) |
| **Evidence artifact** (`verify --evidence`) | Schema-versioned run record with argv + output hashes and a hash chain | ASI09 | MEASURE 1, MEASURE 3 | Art. 12 | [`evidence.rs`](../crates/do-harness/src/evidence.rs) |
| **Hash-chained event log** (`.do-harness/agent_state.db`) | Append-only workflow events with `UNIQUE(seq)` and verified chain | ASI06 | GOVERN 2, MEASURE 3 | Art. 12 | [`repo_workflow.rs`](../crates/db/src/repo_workflow.rs), [`0011_schema_hardening.sql`](../crates/db/migrations/0011_schema_hardening.sql) |
| **Fail-closed semantics** | Deny-by-default on unmet gates, unknown sensors, or DB skew | ASI08, ASI10 | GOVERN 1, MANAGE 1 | Art. 9 (dev-loop) | [`sensors/mod.rs`](../crates/do-harness/src/sensors/mod.rs), [`dbcheck.rs`](../crates/do-harness/src/dbcheck.rs) |

---

## OWASP Agentic Top 10 (ASI 2026 taxonomy)

| Risk ID | Risk Title | Coverage | do-harness Control & Evidence |
|---|---|---|---|
| **ASI01** | Agent Goal Hijack | ⚠️ Partial (Dev-Loop) | Fixture assertions can encode prompt-injection expectations; runtime prompt interception is out of scope. [`eval_assert.rs`](../crates/do-harness/src/eval_assert.rs) |
| **ASI02** | Tool Misuse and Exploitation | ✅ Dev-Loop | Methods validated against the catalog and each subtask gate re-checked in-transaction. [`methods.rs`](../crates/do-harness/src/methods.rs), [`repo_workflow.rs`](../crates/db/src/repo_workflow.rs) |
| **ASI03** | Identity and Privilege Abuse | ⚠️ Partial (Dev-Loop) | Commit identity rules and machine-readable invariants; runtime identity/credential policy is out of scope. [`check-commitlint.sh`](../scripts/check-commitlint.sh), [`plans/invariants.json`](../plans/invariants.json) |
| **ASI04** | Agentic Supply Chain Vulnerabilities | ✅ Dev-Loop | `cargo deny` + RustSec sensors and transitive dependency-direction checks. [`check-deps.sh`](../scripts/check-deps.sh), [`check-audit.sh`](../scripts/check-audit.sh), [`deny.toml`](../deny.toml) |
| **ASI05** | Unexpected Code Execution (RCE) | ✅ Dev-Loop | `#![forbid(unsafe_code)]`, `clippy -D warnings`, LOC cap; eval walkthroughs use **filesystem isolation only** (no seccomp/netns/gVisor — see [`docs/threat-model-eval.md`](threat-model-eval.md)). [`eval_sandbox.rs`](../crates/do-harness/src/eval_sandbox.rs) |
| **ASI06** | Memory and Context Poisoning | ⚠️ Partial (Dev-Loop) | Append-only `workflow_events` + `UNIQUE(seq)`/`UNIQUE(chain_hash)` and `audit-chain` verification; runtime agent memory sandboxing is out of scope. [`0011_schema_hardening.sql`](../crates/db/migrations/0011_schema_hardening.sql), [`audit.rs`](../crates/do-harness/src/audit.rs) |
| **ASI07** | Insecure Inter-Agent Communication | ✅ Dev-Loop | Strongly typed command/event contracts with `deny_unknown_fields`. [`crates/types`](../crates/types) |
| **ASI08** | Cascading Agent Failures | ✅ Dev-Loop | Per-sensor strike counters and fail-fast halt after 3 consecutive failures; blocked sensors are synthesized as failures. [`telemetry.rs`](../crates/do-harness/src/telemetry.rs) |
| **ASI09** | Human-Agent Trust Exploitation | ✅ Dev-Loop | Computational sensors override self-assessment; `task done` rejects unverified claims. [`sensors/mod.rs`](../crates/do-harness/src/sensors/mod.rs), [`repo_workflow.rs`](../crates/db/src/repo_workflow.rs) |
| **ASI10** | Rogue Agents | ✅ Dev-Loop | Terminal transitions require passing gates re-checked inside the command transaction. [`repo_workflow.rs`](../crates/db/src/repo_workflow.rs) |
| **do-harness Traceability extension** | *Non-standard; not an OWASP ASI category* | ✅ Dev-Loop | Hash-chained workflow events and hash-chained evidence artifacts. [`repo_workflow.rs`](../crates/db/src/repo_workflow.rs), [`evidence.rs`](../crates/do-harness/src/evidence.rs) |

---

## NIST AI Risk Management Framework (AI RMF 1.0)

Mappings are **organisational dev-loop support**, not certification:

### GOVERN
- **GOVERN 1 (Policies)** — [`plans/invariants.json`](../plans/invariants.json) persisted via `do-harness seed`, with explicit wontfix decisions recorded.
- **GOVERN 2 (Accountability)** — append-only `workflow_events` and sensor beats in `.do-harness/agent_state.db`; `doctor` surfaces orphan tasks and stale exports.

### MAP
- **MAP 1 & MAP 2 (Context)** — HTN decomposition and the frozen [`plans/methods.json`](../plans/methods.json) catalog make *engineering task* context explicit. This is **not** AI-system risk categorization.
- **MAP 4 (Risks identified)** — static invariant and sensor suites surface code/dependency/structure risks; this narrows the scope of MAP 4 to dev-loop artifacts.

### MEASURE
- **MEASURE 1 (Metrics applied)** — `do-harness metrics` aggregates sensor stats, strike counts, and skill pass-rate history in SQL.
- **MEASURE 2 & 3 (Analysis and tracking)** — `do-harness eval` grades sandboxed walkthroughs against SHA-256 baselines with a pass-rate floor; these are **dev-loop fixture evaluations**, not evaluations of a deployed AI system.

### MANAGE
- **MANAGE 1 (Risk response)** — fail-fast strikes and fail-closed gates; busy/lock retries keep concurrent writers consistent.
- **MANAGE 4 (Monitoring)** — pre-commit/pre-push hooks run the same sensor pack as CI; `maintenance` prunes unbounded history.

---

## EU AI Act Alignment (dev-loop scope only)

These rows map Articles 9/10/12/14/15 to **developer-workflow controls**. They
are not a claim of conformity for any high-risk AI system under Regulation (EU)
2024/1689.

| EU AI Act Article | Requirement | do-harness Implementation & Evidence |
|---|---|---|
| **Article 9** | Risk management system | Continuous `verify` sensors, fail-fast strike halting, and fail-closed task gates. [`sensors/mod.rs`](../crates/do-harness/src/sensors/mod.rs) |
| **Article 10** | Data & data governance | Strongly typed schema contracts and dependency auditing. [`crates/types`](../crates/types), [`check-deps.sh`](../scripts/check-deps.sh) |
| **Article 12** | Technical documentation & record-keeping | Evidence artifacts and hash-chained event logs with enforced uniqueness. [`evidence.rs`](../crates/do-harness/src/evidence.rs), [`repo_workflow.rs`](../crates/db/src/repo_workflow.rs) |
| **Article 14** | Human oversight | `task done` requires passing beats; `eval --bless` records an approver identity in append-only history. [`repo_eval.rs`](../crates/db/src/repo_eval.rs) |
| **Article 15** | Accuracy, robustness and cybersecurity | `cargo check`/`test`/`clippy`, `forbid(unsafe_code)`, LOC caps, SSRF-guarded optional proxy. [`do-harness.toml`](../do-harness.toml), [`crates/guardian-proxy`](../crates/guardian-proxy) |

---

## SOC 2 Trust Services Criteria (dev-loop scope)

| Criterion | Dev-loop mapping | Evidence |
|---|---|---|
| **CC6.1** — Logical access | Managed git hooks and commit-message enforcement; permissions are least-privilege in CI. [`hooks.rs`](../crates/do-harness/src/hooks.rs), [`verify.yml`](../.github/workflows/verify.yml) |
| **CC7.2** — Monitoring | Sensor strikes, `metrics`, and `doctor` orphans/freshness. [`metrics.rs`](../crates/do-harness/src/metrics.rs), [`doctor.rs`](../crates/do-harness/src/doctor.rs) |
| **CC8.1** — Change management | Every change is sensor-gated locally and in CI, with evidence artifacts uploaded per run. [`verify.yml`](../.github/workflows/verify.yml) |
