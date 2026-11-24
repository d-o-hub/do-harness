# Fresh-context review decision protocol (v1)

**Issue:** #499 — measure fresh-context review outcomes and decide
`GO` / `NO_GO` / `INCONCLUSIVE`.

**Version:** 1 (frozen). This protocol is predeclared: it is frozen before
held-out effectiveness results are collected, and it changes only by a
versioned revision that also precedes results. No numeric threshold in this
document may be adjusted after results are collected.

**Corpus:** `benchmarks/fresh-context-decision-v1/` (cases, designs, source
snapshots, and the separate ground-truth oracles).
**Machine-readable thresholds:** `benchmarks/fresh-context-decision-v1/protocol.json`.
**Adjudication machinery:** `scripts/fresh_context_decision/` with wrapper
`scripts/fresh-context-decision.py` (`check-corpus`, `aggregate`,
`--self-test`).

## 1. Dependency and opt-in status

This protocol freezes the measurement design and the adjudication machinery.
The actual paired effectiveness runs are **explicit opt-in external-agent
work** that:

1. depends on the #498 pilot landing (`plans/fresh-context-review-experiment.md`
   and its review adapter) so fresh-context review mechanics exist; and
2. requires real paired model runs (baseline vs. treatment) that are never
   launched from sensors, CI, or any default gate.

Until #498 lands and the pilot feasibility data pins every numeric threshold
in `protocol.json` (`status: "pinned"`), the committed protocol state is
`unpinned` and the decision machinery always yields `INCONCLUSIVE`. This
document's thresholds are placeholders explicitly marked **to be pinned from
the #498 pilot data before held-out runs**.
Reporting and usage provenance reuse the #458 runner contract
(`scripts/workflow_benchmark/runner.py`): the same `token_cost`,
`currency_cost`, `context_bytes`, `adapter_model`, and `usage_reason` labels
(`unmeasured-fixture`, `provider usage unavailable in adapter protocol`), the
same scrubbed environment rule, and the same "missing cost is unavailable,
not zero" accounting. Per-run records additionally reuse the #498 pilot
outcome vocabulary (`success_with_findings`, `success_zero_findings`,
`failure`, `timeout`, `not_run`, `unverified`) and finding dispositions
(`confirmed`, `rejected`, `unresolved`).

## 2. Eligible change classes

Only changes in the three consequential risk classes are eligible:

1. **Authorization boundaries** — delegation, group/permission resolution,
   revocation, power checks, confused-deputy surfaces.
2. **Database migration / data integrity** — schema changes, backfills,
   idempotence, rollback safety, writer blocking.
3. **Public API compatibility** — additive vs. breaking surface changes,
   defaults, deprecation windows, silent behavior changes.

Plus one **routine low-risk control** with no consequential design choice; it
exists to expose unnecessary ceremony and false alarms (a review that
invents ceremony on the control is a recorded false positive, never a
success signal).

Routine fixes and mechanical changes outside these classes are not eligible
case material; they continue through the ordinary workflow.

## 3. Corpus selection

The frozen corpus is seven synthetic, sanitized cases: one flawed and one
independently checked sound design in each risk class, plus the low-risk
control. Each case carries a design document and a minimal source snapshot
from the exact pre-implementation state. Ground truth lives **outside
reviewer inputs**: a per-case `ground-truth.json` oracle that the packet
assembler rejects by construction (fail-closed oracle-leak guard), so no
packet can reference or embed expected findings.

Selection rules:

- Stored ground truth is public in the repository but structurally excluded
  from packet assembly; the self-test proves every packet is clean.
- Cases are synthetic/sanitized; no production code, conversation data, or
  private trace is used.

## 4. Repeated-run count and pairing

- Paired independent sessions: the same task, initial design snapshot, model
  revision/settings, tool capabilities, and total resource cap in both arms.
- **Baseline:** ordinary planning, implementation, deterministic
  verification, and PR-review workflow.
- **Treatment:** the same workflow plus the single fresh-context design
  review from #498 before implementation.
- Run order is counterbalanced per pair (`order_in_pair`), and no findings or
  modifications are carried across arms.
- Repetition: `repeated_runs_per_design_per_arm` (placeholder, unpinned).
  Repeated runs of one design are measurement repeats, **not** independent
  design cases; the distinct design is the sampling unit for all
  uncertainty statements.

## 5. Model/client settings and identities

Every run records: repository commit, design/design revision, packet hash,
prompt/objective identity, adapter identity (hash or name), model version,
client version, reviewer startup/launch evidence, review completion, and
isolation-evidence status (`verified` / `unverified`). Unavailable identities
are recorded as `unverified`, never assumed. Parent conversation is never
supplied; host-injected instructions are recorded as unverified unless native
records prove otherwise.

## 6. Finite resource limits

Per review: timeout 300s, at most 5 findings, 64 KiB output cap, 1 MiB
packet cap (all in `protocol.json` `limits`; these are the same order of
magnitude as the existing workflow-v1 budget and are never silently raised to
fit the experiment). Per-arm and total usage budgets are set by the pinned
thresholds below. Output token/currency cost is recorded with provenance
(`provider-observed`, `adapter-reported`, `estimated`, `unavailable`) and
coverage (`full`, `partial`, `none`).

## 7. Exclusions

A run is excluded from the detection-rate denominators only when it produced
no review output at all due to infrastructure failure (launch failure,
timeout before any output, `not_run`, `unverified` capability). Excluded
runs are retained in the record with their reason and costs; they count
toward the incomplete-run totals reported with the verdict. No run is
silently dropped.

## 8. Stopping rule

- Fixed sample: all eligible designs × repeated runs, per the pinned
  `min_complete_designs` and `repeated_runs_per_design_per_arm`.
- **No early stopping on an observed trend**; the protocol never re-checks
  interim outcomes to decide whether to continue.
- Stops before the fixed sample occur only on budget exhaustion, resource
  caps, or infrastructure failure; partial runs are retained and reported,
  and the verdict falls to `INCONCLUSIVE` when the fixed sample is not met.

## 9. Primary outcome and numeric thresholds (placeholders)

- **Primary outcome:** the count of designs where the treatment arm
  confirms at least one material defect that the paired baseline did not
  find (`new_beyond_baseline`), divided by the number of complete designs
  (advantage rate).
- **Minimum practically worthwhile improvement** (`mwi_advantage_rate`):
  *to be pinned from #498 pilot data before held-out runs* — placeholder
  `null` in `protocol.json`.
- **False-positive/regression tolerance**
  (`false_positive_tolerance_per_design`,
  `regression_tolerance_rate`): *to be pinned from #498 pilot data before
  held-out runs* — placeholders `null`.
- **Maximum review time** (`max_review_time_minutes`) and **maximum usage
  cost** (`max_usage_cost_usd`): *to be pinned from #498 pilot data before
  held-out runs* — placeholders `null`.

## 10. Versioned finding rubric

- **Material defect:** a design error in an eligible class that, if
  implemented as proposed, would violate an explicit constraint or produce a
  consequential failure (authorization bypass/stale enforcement/confused
  deputy; data loss/non-idempotent/destructive rollback/blocked writers;
  silent breaking API change/removal without deprecation). Minor defects
  (style, naming, non-consequential refactors) are recorded but are not
  "material" for the primary outcome.
- **Severity:** `material` or `minor` per defect, frozen in the oracle.
- **Duplicates:** findings that restate another finding in the same review
  (flagged via `duplicate_of`) are counted once; they are recorded, never
  dropped.
- **Previously known issues:** a defect already caught by the paired baseline
  arm is overlap, not treatment value; it is reported as
  `baseline_overlap` and excluded from `new_beyond_baseline`.
- **Unsupported claims:** a finding without concrete affected path, trigger,
  consequence, evidence, or smallest correction is not a valid finding; an
  adjudicator disposition of `rejected` (with evidence) makes it a false
  positive, never a hit.
- **Unresolved findings:** findings that receive no disposition, or a
  `unresolved` disposition, keep the run incomplete for the primary outcome
  and are reported.

## 11. Adjudication

Findings are adjudicated against the frozen requirements and source evidence,
using reproductions, executable contract checks, or the independently checked
defect oracle where possible. The adjudicator is blinded to treatment
identity where feasible. A second model's agreement is not ground truth.
Every confirmed finding must map to an oracle defect; a confirmed mapping to
an unknown defect is a data error and fails the machinery closed.

## 12. Decision rule

Let `n` = complete designs (both arms completed a review). The verdict is
computed only from predeclared thresholds:

- `INCONCLUSIVE` when:
  - thresholds are not pinned (`protocol.json` `status != "pinned"`), or
  - `n < min_complete_designs` (insufficient sample), or
  - isolation evidence is missing for any treatment run, or
  - review timing or usage cost was not recorded for any treatment run
    (missing cost is unavailable, not zero), or
  - the advantage-rate estimate straddles `mwi_advantage_rate` (the 95%
    Wilson interval contains both the bar and a materially lower effect).
- `NO_GO` when, with a sufficient sample:
  - false positives exceed `false_positive_tolerance_per_design × n`, or
  - the regression rate on implementation runs exceeds
    `regression_tolerance_rate`, or
  - median treatment review time exceeds `max_review_time_minutes`, or
  - treatment usage cost exceeds `max_usage_cost_usd`, or
  - the upper Wilson bound is below `mwi_advantage_rate` (effect confidently
    below the bar).
- `GO` when, with a sufficient sample, no criterion above fails, the
  advantage rate meets `mwi_advantage_rate`, and the interval lower bound is
  above zero.

A single impressive finding, or any successful demonstration on one design,
is never the adoption criterion. Claims of reduced implementation rework are
made only for designs where both arms actually implemented and underwent
equivalent independent checks; otherwise the verdict is detection-only and is
labeled as such.

## 13. Record format

Per design and per run, records capture:

- confirmed material defects found before implementation, including findings
  beyond the ordinary workflow (`new_beyond_baseline`);
- missed known defects, false positives, duplicate findings, and useful
  findings already found by the baseline (`baseline_overlap`);
- confirmed regressions and new errors introduced while addressing review
  findings;
- for implementation runs: final task success, escaped defects, repair
  cycles, and later review rework;
- total elapsed time and human adjudication time (packet preparation,
  reviewer startup, review, disposition, fixes, implementation,
  verification);
- available tokens/currency with provenance and coverage;
- exact repository/design/prompt/adapter/model/client identities, review
  completion, and isolation-evidence status.

Failed launches, timeouts, missing reads, budget exhaustion, and unresolved
findings are retained. The aggregation script
(`fresh-context-decision.py aggregate --records DIR --protocol FILE`)
computes the verdict above, with `INCONCLUSIVE` on insufficient samples, and
unit tests prove the `GO` / `NO_GO` / `INCONCLUSIVE` paths with synthetic run
records.

## 14. Validation and CI

The deterministic checks (`fresh-context-decision.py --self-test`) validate:
corpus coverage (classes × variants + control), predeclared expected findings
per case, oracle-leak rejection, record schema, adjudication, and verdict
paths. Ordinary CI stays offline; no model run and no recurring paid
evaluation is introduced by this protocol. The existing computational
verification and freshness gates remain unchanged.

## 15. Recording and follow-up

Results land in the experiment record started in #498, with compact sanitized
per-run artifacts and the complete case selection. The three outcomes close
this issue:

- `GO` — open a separately scoped implementation issue for the smallest
  useful opt-in integration and state which risk classes benefit.
- `NO_GO` — retain the result and remove pilot machinery with no continuing
  value.
- `INCONCLUSIVE` — state the specific gap; retain the ordinary workflow
  without adding a mandatory step.

A favorable result for one reviewer step never justifies the full EGM
multi-stage process.

## History

| Version | Change | When |
| --- | --- | --- |
| 1 | Frozen predeclared protocol, corpus, and adjudication machinery | Before held-out runs (PR for #499) |
