# `plans/` — policy records, epics, and archived evidence

> **Status:** index of a maintained directory; keep it current when a record is
> added, archived, or retired.

This directory is the repository's durable planning record (AGENTS.md §1). It
holds three kinds of file, and only the first kind is a live contract:

1. **Machine-readable gates** — read by the CLI or a sensor. Editing one changes
   behaviour, so they are reviewed like code.
2. **Live epics** — the reviewed design and decision record for work that is
   still governing.
3. **`archive/`** — closed experiment, release, and progress records kept as
   evidence. They are classified `historical` in `docs/contracts.toml`, so their
   links and examples are outside the maintained-document gate.

## Gates (do not rename)

| File | Consumed by |
| --- | --- |
| `invariants.json` | `do-harness seed`, the `check-invariants` sensor |
| `methods.json` | `do-harness task add --method`, the HTN runtime |
| `baselines.json` | `verify --record`'s `FINDINGS: <n>` ratchet |
| `dora.json` | `do-harness dora` |
| `ci-workflow-costs.json` | `scripts/ci-workflow-costs.sh` |
| `pr-merge-policy.toml` | `do-harness pr` readiness checks |

A rename here is not a move, it is an outage: the sensor that owns the file
fails closed rather than falling back to a default.

## Live records

| File | Purpose |
| --- | --- |
| `agt-governance-epic.md` | Agent Governance Toolkit tracking and promotion criteria |
| `mcp-conformance-epic.md` | MCP conformance slices for `guardian-proxy` |
| `development-signals-epic.md` | Signal sets, change-aware applicability, evidence freshness |
| `distribution-epic.md` | Release distribution and external adoption |
| `pr-triage-skill-epic.md` | `pr-triage` skill; semantic-routing and refinement verdicts |
| `libsql-distribution.md` | Vendored libSQL line and its advisory watch |
| `serde-yaml-migration.md` | The live reason `RUSTSEC-2024-0449` is ignored |
| `ci-concurrency-prevention.md` | CI concurrency, timeout, and OOM guardrails |
| `ci-scope-measurement.md` | Measurement behind change-selection scoping |
| `verify-change-selection-proposal.md` | Proposed CLI fixes for `verify --changed` |
| `sync-adopter-validation.md` | `sync` validated against an external adopter |

## Archive

`archive/` holds the closed records: the 0.4.0 roadmap and its progress record,
the 2026-10-15 codebase analysis and its follow-ups, the October issue batch, and
the three rejected or deferred experiments (affected-component selection,
workflow-adopter corpus, multi-candidate refinement).

Read them for *why* a decision went the way it did. Do not reopen one as missing
work without checking its recorded `Status:` line first — several argument
tables in the roadmap exist only to record that an earlier queue description was
already resolved.

## Conventions

- Every file opens with an H1 and a `> **Status:**` line. `closed`, `active`,
  `decided`, `proposed`, and `recorded` are the states in use.
- Renaming or archiving a file means fixing every reference *and* any
  `file.md:NN-NN` line citation: `docs-links` proves a link resolves, but it
  does not follow a line range, so a stale `:13-25` rots silently.
- `scripts/check-skills-docs.sh` scans only `plans/*-epic.md`. An epic moved into
  a subdirectory silently stops being checked for numbered-heuristic citations.
