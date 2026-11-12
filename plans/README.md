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

| File | Consumed by | If it is missing |
| --- | --- | --- |
| `invariants.json` | `do-harness seed`, the `check-invariants` sensor | Fail closed: `check-invariants.sh` reports `does not exist` |
| `methods.json` | `do-harness task add --method`, the HTN runtime | Silent fallback: `methods.rs` uses the embedded catalog, so a local edit disappears |
| `baselines.json` | `verify --record`'s `FINDINGS: <n>` ratchet | Silent fallback: `Baselines::load` returns the default, so the ratchet stops ratcheting |
| `dora.json` | `do-harness dora` | Fail closed: `dora::policy::load` errors naming the path |
| `ci-workflow-costs.json` | `scripts/ci-workflow-costs.sh` | Fail closed: `FileNotFoundError` |
| `pr-merge-policy.toml` | `do-harness pr` readiness checks | Reports `UNKNOWN`, never a pass |

A rename here is not a move, it is a change in gate strength, and which way it
changes depends on the file: three fail closed, two fall back to a built-in
default, and one turns the merge decision into `UNKNOWN`. The two that fall back
are the dangerous ones — a rename there is silent, and nothing red will tell
you. Rename any of them in a reviewed change that also updates the consumer.

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
workflow-adopter corpus, multi-candidate refinement). `refinement-raw-data.json`
is the raw measurement output behind the last of those — the 60 runs whose
`VERDICT=DEFER` it records — so it lives beside the record that cites it, not
with the gates.

Read them for *why* a decision went the way it did. Do not reopen one as missing
work without checking its recorded `Status:` line first — several argument
tables in the roadmap exist only to record that an earlier queue description was
already resolved.

## Conventions

- Every file opens with an H1 and a `> **Status:**` line. `closed`, `recorded`,
  `active`, `decided`, and `proposed` are the states in use; a few older epics
  open with a bare `**Status:**` line instead, which is accepted but not the
  form to copy.
- Renaming or archiving a file means fixing every reference *and* any
  `file.md:NN-NN` line citation: `docs-links` proves a link resolves, but it
  does not follow a line range, so a stale `:13-25` rots silently.
- `scripts/check-skills-docs.sh` scans only `plans/*-epic.md`. An epic moved into
  a subdirectory silently stops being checked for numbered-heuristic citations.
