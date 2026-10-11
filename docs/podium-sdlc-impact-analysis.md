# Impact Analysis: "Rewiring the SDLC After Code Became Cheap" vs. `do-harness`

## Executive Summary

`do-harness` is a Rust-based CLI tool and developer harness that pairs feedforward guidance (`AGENTS.md`, `.agents/skills/`) with feedback sensors (deterministic computational checks in `do-harness.toml`) to verify software changes before commit, push, or merge. It is used by autonomous AI coding agents (e.g., DeepSeek Harness, Claude, Codex, or custom agents) and human developers across Rust, Node, Web, and Generic stacks to maintain codebase invariants, trace execution workflows, enforce compliance policies, and assess PR merge readiness.

This report evaluates whether the insights from Podium's article, *"Rewiring the SDLC After Code Became Cheap"* (Rishit Shah, July 2026), have actionable impact for the `do-harness` codebase—both as a product/roadmap for its users and for its internal development process.

---

## Pillar 1: Skillifying How Engineers Work With Agents (Skills & Hooks)

### Verdict
**Direct Impact (Product/Roadmap)** | **Partial Impact (Internal Process)**

### Evidence
- **CLI Scaffolding & Hook Management**: `do-harness init` (`crates/do-harness/src/init/`), `do-harness hook install` (`crates/do-harness/src/hooks/`, `crates/do-harness/src/hook_script.rs`), and `do-harness.toml`.
- **Skill Lifecycle & Discovery Primitives**: `do-harness skills suggest` (`crates/do-harness/src/skills/suggest.rs`), `do-harness skills drift` (`crates/do-harness/src/skills/drift.rs`), `do-harness distill` (`crates/do-harness/src/distill.rs`), `do-harness learn` (`crates/do-harness/src/learn/`), and `do-harness eval` (`crates/do-harness/src/eval/`).
- **Template Synchronization**: `.agents/skills/` and `crates/do-harness/templates/skills/` enforced by `scripts/check-skills-sync.sh`.

### Analysis & Gaps
Podium describes an internal "skills marketplace" distributing standardized agent skills and git hooks across teams via an internal CLI. `do-harness` already implements local skill scaffolding, discovery (`skills suggest`), drift detection (`skills drift`), and trace distillation (`distill`). However, `do-harness` currently only manages local or repo-bundled skills.

### Follow-Up Opportunities (Ranked)
1. **Remote Skill Distribution & Registry (`do-harness skills install <url|repo>`)**
   - *Description*: Expand `do-harness skills` to fetch, pin, and update shared skill packages from git repositories or HTTPS manifests, updating `skills drift` checks accordingly.
   - *Effort*: Medium (~3–4 days)
2. **Skill-Set Bundles in `do-harness init`**
   - *Description*: Allow `do-harness init --skills <bundle>` to scaffold stack-specific or org-specific skill packs alongside language sensor packs.
   - *Effort*: Low (~1–2 days)
3. **Internal Process: Auto-Distill Recurring Traces**
   - *Description*: Run `do-harness learn --draft` in internal maintenance workflows to automatically suggest updates to `.agents/skills/` when sensors fire repeatedly.
   - *Effort*: Low (~1 day)

---

## Pillar 2: Code Review: Turning Engineering Judgment Into Scalable Checks

### Verdict
**Direct Impact (Product/Roadmap)** | **No Impact (Internal Process)**

### Evidence
- **PR Review & Readiness Engine**: `do-harness pr review` (`crates/do-harness/src/pr/review.rs`), `do-harness pr ready` (`crates/do-harness/src/pr/readiness.rs`), and `do-harness pr waivers` (`crates/do-harness/src/pr/waivers.rs`).
- **Anti-Slop Guidelines**: `.agents/skills/skill-creator/references/anti_ai_slop.md`.
- **Triage & Routing**: `integrations/pr-triage/kev/` and `crates/do-harness/tests/pr_routing.rs`.
- **Optional Governance Sidecar**: `crates/guardian-proxy/src/proxy.rs` (gated via `agt-governance`).

### Analysis & Gaps
Podium built an AI review system combining security, architecture, and de-slopification checks, reaching 70% PR auto-approval for low-risk changes. `do-harness` currently provides `pr review` (which emits semantic residual units that automation could not prove) and `pr ready` (which evaluates merge readiness and blocker conditions). However, `do-harness` lacks a policy rule engine to evaluate change risk tiers and output an explicit, deterministic `auto_approve: true/false` decision.

### Follow-Up Opportunities (Ranked)
1. **Risk-Stratified Auto-Approval Policy Engine in `pr ready`**
   - *Description*: Extend `do-harness pr ready` / `pr review` to support configurable policy rules (e.g., zero residual units + passing verification set + no schema/security file changes = `auto_approve: true`).
   - *Effort*: Medium (~3–5 days)
2. **First-Class De-Slopification Sensor (`check-slop.sh`)**
   - *Description*: Promote `anti_ai_slop.md` from a passive reference guide into an active sensor script or rule checking diffs for redundant comments, unneeded boilerplate, and hallucinated abstractions.
   - *Effort*: Low-Medium (~2 days)

---

## Pillar 3: WALL-E: Autonomous Cloud Agent for Bug Fixing and Triage

### Verdict
**No Impact / Out of Scope (Product/Roadmap & Internal Process)**

### Evidence
- **CLI Architecture Scope**: `README.md`, `crates/do-harness/src/main.rs`.
- **Local Workflow Store**: `crates/do-harness/src/task/` and `crates/types/src/task.rs` (local SQLite database `.do-harness/agent_state.db`).
- **Runtime Scope Boundary**: `README.md` explicitly notes: *"do-harness itself stays a dev-loop harness (no traffic proxy)"*.

### Analysis & Gaps
Podium's WALL-E is an autonomous background worker running on cloud infrastructure that monitors issue trackers (Jira/Zendesk), reproduces bugs, provisions preview environments, and creates PRs.

`do-harness` is explicitly designed as a *local/CI execution and verification harness*, not an autonomous cloud agent orchestrator or issue-tracker polling service. While `do-harness` tracks task states locally (`task add`/`advance`/`done`), hosting cloud workers or issue integrators belongs to external agent platforms (e.g. DeepSeek Harness, Cordis, or custom worker clouds) that *use* `do-harness` as their execution substrate (`integrations/deepseek-harness/`). Re-architecting `do-harness` to become an issue-ingesting cloud daemon would contradict its design principles (`docs/compliance.md`).

---

## Pillar 4: CI: From Developer Babysitting to Agentic Validation (Agentic CI Follow-Through)

### Verdict
**Direct Impact (Product/Roadmap)** | **Partial Impact (Internal Process)**

### Evidence
- **CI Failure Diagnosis**: `do-harness ci-explain` (`crates/do-harness/src/ci_explain/`, `crates/do-harness/src/commands/ci_explain.rs`).
- **Error Signatures & Striking**: `crates/do-harness/src/errors.rs` and `crates/types/src/error_signature.rs`.
- **PR Rerun Recommendations**: `crates/do-harness/src/pr/readiness.rs` (emits `gh run rerun <id> --failed`).
- **Agent Integration Seams**: `integrations/deepseek-harness/lib/harness.mjs` and `integrations/deepseek-harness/lib/gate.mjs`.

### Analysis & Gaps
Podium's WALL-E monitors CI runs, interprets failures, applies fixes, and retries jobs toward a green merge state. `do-harness` already features `ci-explain` to diagnose GitHub Actions runs and map failed jobs to exact local sensor repro commands. However, `ci-explain` output is currently optimized for human console reading rather than structured ingestion by autonomous coding agents.

### Follow-Up Opportunities (Ranked)
1. **Agent-Structured Remediation Payload in `ci-explain --format json`**
   - *Description*: Enhance `ci-explain` JSON output to include explicit agent remediation fields (`target_files`, `failing_sensor`, `repro_command`, `suggested_fix_context`) so coding agents can programmatically consume CI failures and execute targeted fixes.
   - *Effort*: Medium (~2–3 days)
2. **Automated CI Failure-to-Trace Conversion (`trace add --from-ci`)**
   - *Description*: Allow `do-harness trace add` to ingest `ci-explain` failure payloads directly, enabling agents to record CI fix iterations and distill them into reusable skills (`do-harness distill`).
   - *Effort*: Low-Medium (~2 days)
3. **Internal Process: CI Actionable Failure Summaries**
   - *Description*: Add a step in `.github/workflows/verify.yml` that invokes `do-harness ci-explain` on job failure and appends exact sensor repro commands to `$GITHUB_STEP_SUMMARY`.
   - *Effort*: Low (~0.5 days)

---

## Overall Verdict

**Does this article change anything about how `do-harness` should be built or run?**

**No fundamental architectural changes are needed.** The article strongly validates `do-harness`'s core philosophy: as code generation becomes cheap, downstream validation, review, and CI verification become the primary bottlenecks.

`do-harness` already contains the core primitives for all four pillars described by Podium. Rather than pivoting or adding autonomous cloud worker daemons, `do-harness` should focus on sharpening its existing primitives to serve agentic loops better:
1. **Remote Skill Distribution**: Expanding `skills drift`/`suggest` into remote package management.
2. **Auto-Approval Gating**: Adding deterministic risk rules to `pr ready` to allow automated merging of low-risk PRs.
3. **Agentic CI Diagnostics**: Enhancing `ci-explain` with machine-readable remediation hints for autonomous agents.
