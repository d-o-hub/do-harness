# Dependency checkpoints (dated)

> **Status:** active — reviewed at each checkpoint, not on every push
> **Recorded:** `plans/` · **Issues:** #568

Three supply-chain items depend on dates, not on a code change. They are
tracked here so a date passing is visible in the repository rather than only in
an open issue. A checkpoint that arrives with no decision is a failure of this
record, not a deferral.

## Checkpoints

| Date | Action | Evidence / command | Outcome recorded in |
| --- | --- | --- | --- |
| 2027-02-15 | Re-run the AGT GA watch and decide: migrate to `agentmesh` **or** remove the `agt-governance` feature (keeping fail-closed `guardian-proxy` mediation) | `bash scripts/check-agt-ga.sh`; `.github/workflows/agt-ga-watch.yml` runs it monthly and opens an issue only on `VERDICT=GA` | `plans/agt-governance-epic.md`, this table |
| 2027-02-15 | If neither path is possible, re-date the `RUSTSEC-2024-0449` ignore **with a reason**; never let it expire silently | `deny.toml` comment + `scripts/check-audit.sh --ignore` | `plans/serde-yaml-migration.md`, `deny.toml` |
| 2027-03-25 | Hard expiry recorded for `RUSTSEC-2024-0449` in `deny.toml`; the migration must have landed or the ignore must have been re-dated at the 2027-02-15 checkpoint | `cargo deny check advisories` | `deny.toml` |
| 2027-03-25 | Retire `RUSTSEC-2026-0543` (`chrono` unmaintained, disclosed 2026-09-14) by upgrading `rmcp` to a release that drops chrono; otherwise re-date the ignore with a reason | `cargo audit` / `cargo deny check advisories`; `cargo tree --all-features -i chrono` | `deny.toml`, `scripts/check-audit.sh` |
| Monthly (already automated) | Confirm the vendored libSQL fork's advisory watch still runs and covers `vendor/libsql` (`do-harness-libsql`) | `.github/workflows/scheduled.yml` audit/`cargo deny` jobs; `plans/libsql-distribution.md` | `plans/libsql-distribution.md` |

## Current state (2026-11-16)

- Workspace pins `agent-governance = "=3.2.2"`, retired upstream in favour of
  `agentmesh`. Latest live release at the 2026-10-14 review: `agentmesh` 4.0.0,
  still Public Preview (`VERDICT=NOT_GA`).
- Migrating is a breaking API change (`serde_yaml` → `agentmesh::policy_data`)
  and is gated by `chore-agt-promotion` in `plans/agt-governance-epic.md`,
  currently `hold`.
- `RUSTSEC-2024-0449` (`unsafe-libyaml` via `serde_yaml` 0.9) is unmaintained,
  not a reported vulnerability; it is ignored in `deny.toml` with the
  2027-03-25 revisit date and in `scripts/check-audit.sh`.
- The vendored libSQL fork is covered by the scheduled audit job; no separate
  action is pending.
- `RUSTSEC-2026-0543` (`chrono` 0.4.45 unmaintained, advisory 2026-09-14) is
  ignored for the same reason class: it is reachable only through `rmcp`'s
  `schemars` support in guardian-proxy's optional `mcp-surface` feature, and no
  default build links it. The ignore carries its own reason in `deny.toml` and
  `scripts/check-audit.sh`, so a bare expiry cannot outlive the rationale.

## Escalation

If the 2027-02-15 checkpoint cannot reach a decision, extend the ignore with a
new dated reason **in the same commit** that records why migration and removal
were both rejected. An undated ignore or a silently expired one is a red
supply-chain state, not a warning.
