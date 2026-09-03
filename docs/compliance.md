# Compliance mapping

`do-harness` enforces deterministic, computational controls over the agent development loop. This document maps those controls to recognized AI assurance frameworks: the **OWASP Agentic Top 10 (2026)**, the **NIST AI Risk Management Framework (AI RMF 1.0)**, and the **EU AI Act**.

## Scope & Positioning

`do-harness` is a **dev-loop verification harness** (feedforward guides + feedback sensors, workflow gates, tamper-evident logs, and evidence artifacts). It is **not** a runtime policy engine or inline proxy (such as Microsoft Agent Governance Toolkit / AGT).

Accordingly, `do-harness` claims compliance coverage strictly for **build-time, workflow-level, and dev-loop verification controls**.

> **Adjacent optional runtime:** `crates/guardian-proxy` is a separate, off-by-default, fail-closed sidecar (requires `agt-governance` feature). When enabled it reuses `ProxyMediator::decide`/`AgtGate::check` for tool-call mediation and exposes it over HTTP (`axum` `GET /health`, `GET /metrics`, `POST /mcp/tools/call`), with every decision appended to an optional hash-chained JSONL audit log (`AuditLog`, `SHA-256(prev|payload)`, tamper-evident on reopen) and counted in in-memory observability counters (`ProxyMetrics`: `allow`, `deny`, `mediator_errors`, `upstream_ok`, `upstream_failures`, `audit_write_failures`; counters never affect decisions). It is **not** part of the dev-harness compliance boundary above; runtime claims for the proxy should be evaluated separately.

---

## High-Level Control Matrix

| do-harness Control | Mechanism | OWASP Agentic Top 10 | NIST AI RMF | EU AI Act |
|---|---|---|---|---|
| **Computational sensors** (`verify`) | Deterministic checks strictly supersede LLM self-assessment | ASI04, ASI05, ASI09, Traceability | MEASURE 1, MEASURE 2 | Art. 15 (Accuracy & Robustness) |
| **Task-completion gate** (`task done`) | Refuses task completion until `verify --record` passes named sensor | ASI02, ASI08, ASI09, ASI10 | MANAGE 1, MANAGE 4 | Art. 14 (Human Oversight) |
| **Evidence artifact** (`verify --format json`, `task export`) | Machine-readable, reproducible run record | ASI09, Traceability | MEASURE 1, MEASURE 3 | Art. 12 (Record-keeping) |
| **Hash-chained event log** (`.do-harness/agent_state.db`) | Tamper-evident append-only audit trail of workflow events | ASI06, Traceability | GOVERN 2, MEASURE 3 | Art. 12 (Technical Documentation & Record-keeping) |
| **Fail-closed semantics** | Deny-by-default on unmet preconditions or sensor failures | ASI08, ASI10 | GOVERN 1, MANAGE 1 | Art. 9 (Risk Management System) |

---

## OWASP Agentic Top 10 (ASI 2026 Taxonomy)

The OWASP Top 10 for Agentic Applications (2026) defines 10 primary risk categories (ASI01–ASI10) plus traceability extensions.

| Risk ID | Risk Title | Coverage | do-harness Control & Evidence |
|---|---|---|---|
| **ASI01** | Agent Goal Hijack | ⚠️ Partial (Dev-Loop) | Dev-loop verification sensors validate prompt injection test suites and fixture baselines before release. *Runtime prompt interception is out of scope.* |
| **ASI02** | Tool Misuse and Exploitation | ✅ Full (Dev-Loop) | Workflow gates (`task add/advance/done`) validate methods against the strict method catalog (`plans/methods.json`). Sensors enforce schema and API invariants. |
| **ASI03** | Identity and Privilege Abuse | ⚠️ Partial (Dev-Loop) | Git hook integration (`check-commitlint.sh`) and workspace invariants (`plans/invariants.json`) enforce identity and commit rules during development. |
| **ASI04** | Agentic Supply Chain Vulnerabilities | ✅ Full (Dev-Loop) | Dependency direction linting (`check-deps.sh`) and `cargo deny` sensors prevent compromised or unapproved external dependencies from entering the build. |
| **ASI05** | Unexpected Code Execution (RCE) | ✅ Full (Dev-Loop) | Static sensors (`clippy -D warnings`, `#![forbid(unsafe_code)]`, and `check-loc.sh`) restrict unsafe code constructs, unverified execution, and complexity spikes. |
| **ASI06** | Memory and Context Poisoning | ⚠️ Partial (Dev-Loop) | Append-only event log (`workflow_events`) and libSQL state recording ensure dev-loop state changes are tamper-evident. *Runtime agent memory sandboxing is out of scope.* |
| **ASI07** | Insecure Inter-Agent Communication | ✅ Full (Dev-Loop) | Strongly typed Rust contracts (`crates/types`) enforce command/event schema invariants across HTN planning and task decomposition. |
| **ASI08** | Cascading Agent Failures | ✅ Full (Dev-Loop) | Per-sensor strike counters and fail-fast recovery (`errors list/clear`) halt execution after 3 consecutive failures to prevent cascading errors. |
| **ASI09** | Human-Agent Trust Exploitation | ✅ Full (Dev-Loop) | Automated computational sensors strictly override agent self-assessment. `task done` refuses completion claims without verified sensor passes. |
| **ASI10** | Rogue Agents | ✅ Full (Dev-Loop) | Fail-closed workflow gates deny agent task completion when preconditions or sensor beats are unmet. |
| **AGT Extension** | Agent Traceability | ✅ Full | Merkle-like hash-chained workflow event log (`workflow_events`) and structured JSON run reports (`verify --format json`) provide cryptographic auditability. |

---

## NIST AI Risk Management Framework (AI RMF 1.0)

Mapping to the four core functions of NIST AI RMF 1.0 (NIST AI 100-1):

### 1. GOVERN (Policies, Processes, and Procedures)
- **GOVERN 1 (Policies in place)**: Machine-readable architecture invariants (`plans/invariants.json`) seeded into libSQL (`seed_invariants`) provide executable policies.
- **GOVERN 2 (Accountability structures)**: Append-only event stream (`workflow_events`) and recorded sensor beats (`.do-harness/agent_state.db`) provide tamper-evident developer/agent accountability.

### 2. MAP (Context and Risk Identification)
- **MAP 1 & MAP 2 (Context & Categorization)**: HTN task decomposition (`plans/tasks.json`, methods catalog) structures agent activities into explicit, categorized subtasks with declared preconditions.
- **MAP 4 (Risks identified)**: Static invariants and sensor suites continuously surface code, dependency, and structural risks during development.

### 3. MEASURE (Assessment, Analysis, and Tracking)
- **MEASURE 1 (Metrics applied)**: `do-harness metrics` tracks sensor success rates, strike counts, and skill evaluation pass-rate history.
- **MEASURE 2 & 3 (AI Systems Evaluated & Risk Tracking)**: `do-harness eval` benchmarks hermetic walkthroughs against SHA-256 baseline baselines (`eval --bless`) with a pass-rate bar ratchet.

### 4. MANAGE (Risk Response and Monitoring)
- **MANAGE 1 (Risk Response)**: Fail-fast strike policy halts execution after 3 consecutive sensor failures; fail-closed gates block invalid state transitions.
- **MANAGE 4 (Risks Monitored)**: Continuous dev-loop verification via pre-commit and pre-push git hooks (`do-harness hook install`).

---

## EU AI Act Alignment

Verified against official EU AI Act regulatory requirements for high-risk AI systems and general-purpose AI models:

| EU AI Act Article | Requirement | do-harness Implementation & Evidence |
|---|---|---|
| **Article 9** | Risk management system | Continuous dev-loop sensor evaluation (`do-harness verify`), fail-fast strike halting, and fail-closed task completion gates. |
| **Article 10** | Data & data governance | Strongly typed schema contracts (`crates/types`) and dependency auditing (`check-deps.sh`, `deny.toml`) ensure workspace data integrity. |
| **Article 12** | Technical documentation & Record-keeping | Automatic generation of machine-readable evidence artifacts (`verify --format json`, `task export`) and hash-chained event logs (`workflow_events`). |
| **Article 14** | Human oversight | `task done` gate refuses agent self-certification; human/system verification requires passing computational sensor beats before task completion. |
| **Article 15** | Accuracy, robustness and cybersecurity | Automated feedback sensors (`cargo check`, `cargo test`, `clippy -D warnings`, `#![forbid(unsafe_code)]`, LOC caps) enforce deterministic quality standards. |
