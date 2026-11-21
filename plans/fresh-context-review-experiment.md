# Fresh-context review: opt-in pilot experiment (issue #498)

Status: pilot in progress — feasibility smoke-test run, effectiveness and
adoption undecided.

## Purpose and hypothesis

The ordinary workflow supports proportional planning, uncertainty spikes,
deterministic verification, and semantic PR review, but none of them require
a separate reviewer with **no access to the author's conversation** before
implementation. For substantial authorization changes, database migrations,
or public API changes, an author can carry an unstated assumption into both
the design and the implementation.

Hypothesis: a fresh-context reviewer — one process launched with only a
frozen design packet, no parent conversation — can expose such an assumption
earlier. This is a bounded hypothesis test. It does not establish a current
correctness defect, and it does not justify full Elephant-Goldfish Model
(EGM) integration.

Comparative value and adoption remain **undecided**; this record only
establishes feasibility of the packet/launch/record mechanics and the
predeclared fixture outcomes.

## Eligibility rules

The pilot is optional and focused. A change is eligible **only** when all of
the following hold:

1. The change carries a consequential, unresolved design choice — meaning
   the author cannot yet point to a decided contract for the behavior.
2. The design choice sits in one of exactly three risk areas:
   - authorization (who can do what, and revocation),
   - durable data / migrations (schema, backfill, data integrity),
   - public compatibility (API, CLI surface, file formats, behaviors other
     consumers depend on).
3. The design is frozen at a pinned source revision before the review starts
   (protocol step 1).

Routine fixes and mechanical changes continue through the ordinary workflow;
they are never eligible. The reviewer does not replace design work — it
reviews a frozen candidate contract. A change whose design is already
decided does not qualify: there is no unresolved choice for a fresh reviewer
to expose.

## Review protocol

1. **Identify and freeze.** State the concrete design risk, freeze the
   candidate design and source revision before implementation.
2. **Prepare the packet.** Objective, intended behavior, explicit
   constraints, proposed contract, relevant source/test/document paths with
   revisions, and open questions. Record document/source revisions and the
   packet identity.
3. **Launch one fresh reviewer** with no inherited parent conversation. The
   intended contract and decision rationale live in the design document
   itself. The packet never carries the author's chat transcript, private
   deliberation, or a summary telling the reviewer which conclusion to
   reach.
4. **Let the reviewer inspect** the listed sources and immediately necessary
   callers or contracts. Record material additional reads and missing
   inputs; a narrow packet must not prevent discovery of a relevant
   dependency.
5. **Use read-only review capabilities** where the host supports them. Keep
   review separate from implementation. Record host/client/model versions
   and inherited project/user instructions; no parent conversation does not
   imply absence of host-injected context.
6. **Check isolation on synthetic data** with a parent-only marker probe and
   available native launch/read/completion records. A reviewer claiming to
   be isolated is insufficient evidence; mark unavailable evidence as
   unverified.
7. **Findings** are concise, with affected contract or path, trigger,
   concrete consequence, supporting evidence, and smallest correction. At
   most five material findings, deduplicated. Zero findings are allowed.
   Timeouts, failed launches, missing sources, and incomplete reviews are
   preserved explicitly.
8. **Dispose** each finding as confirmed, rejected with evidence, or
   unresolved. Product trade-offs remain explicit decisions. Apply
   confirmed corrections through the normal implementation and verification
   workflow.
9. **Predeclare finite limits**; use one review pass without an automatic
   convergence loop. An unavailable fresh-context capability yields
   `not run` or `unverified`, never a successful review.
10. **Keep existing gates in force.** `verify`, evidence-freshness, PR
    review, and completion requirements remain mandatory. A model review
    never supplies a sensor pass and never authorizes a merge.

## Packet format

A packet is the sole input to the fresh reviewer. It is a JSON object:

| Field | Meaning |
| --- | --- |
| `schema_version` | Packet schema version (currently `1`). |
| `review_id` | Opaque identifier for the review (e.g. `issue-498-authz`). |
| `objective` | The concrete design risk in one or two sentences. |
| `intended_behavior` | Expected observable behavior. |
| `constraints` | Explicit constraints (revocation immediacy, no schema change, etc.). |
| `proposed_contract` | The candidate contract (signatures, schemas, semantics). |
| `open_questions` | Questions the author genuinely has not decided. |
| `paths` | `source` / `tests` / `docs` path lists. |
| `revisions` | Pinned commit, design-doc path + blob, per-path blob SHAs. |
| `limits` | `timeout_s`, `max_findings`, `max_output_bytes`. |
| `packet_hash` | Identity: SHA-256 over canonical JSON of all other fields. |
| `reviewer_note` | Fixed statement that the contract/rationale live in the design doc, and no author conclusion is supplied. |

- **Canonicalization**: sorted keys, compact separators, UTF-8.
  Byte-for-byte stable serialization for hash and record comparisons.
- **Revision pinning**: every listed file is resolved to its git blob at
  the pinned commit; a missing file at that revision is a hard adapter
  error, never a silently dropped input.
- **Packet identity**: `packet_hash` changes if any input field changes,
  including the pinned revision or any blob SHA.

## Finding format and outcome states

Each finding is exactly:

| Field | Meaning |
| --- | --- |
| `id` | Stable label (`F1`…`F5`). |
| `contract_or_path` | Affected contract or path. |
| `trigger` | What happens to trigger the failure. |
| `consequence` | Concrete failure, with the impacted behavior. |
| `evidence` | Specific source or doc evidence (path:line, construct). |
| `smallest_correction` | The minimal change. |

Limits: at most 5 material findings; related findings are deduplicated;
zero findings are allowed. Out-of-limit findings are counted and recorded as
truncated, never silently dropped.

Outcomes are distinct and exhaustive:

| Outcome | Meaning |
| --- | --- |
| `success_with_findings` | Reviewer completed within limits; 1–5 findings. |
| `success_zero_findings` | Reviewer completed within limits; zero findings. |
| `failure` | Launch/parse/reviewer error, or isolation marker leaked. |
| `timeout` | Reviewer exceeded the predeclared deadline (killed). |
| `not_run` | Capability unavailable (no reviewer configured, or reviewer declared itself unavailable). |
| `unverified` | Review completed but isolation evidence incomplete (e.g. external reviewer without a parent-only marker probe). |

An unavailable or unverified capability is **never** a success.

## Isolation-evidence rules

- A fresh reviewer is launched as a new process with a scrubbed environment
  (PATH/HOME/locale/temp only — no session, agent, credential, or tool
  state). The packet is the only input; there is no parent conversation.
- **Parent-only marker probe**: before a review, the operator places a
  unique marker string in the author's/parent's conversation only. The
  packet never contains it. After the review, the adapter scans the
  reviewer's captured output for the marker. Finding it means parent context
  reached the reviewer — isolation violated. The probe is only meaningful
  when the marker is unique and never passed to the reviewer by the adapter.
- **Native records**: process spawn time, command hash, and scrubbed
  environment key list are recorded as launch evidence.
- **Host-injected context**: no parent conversation does not imply absence
  of host-injected instructions (e.g. a model service's system prompt).
  Where those cannot be observed, the record says so and isolation is
  marked `unverified` unless native records prove otherwise.
- A reviewer claiming to be isolated is insufficient evidence.

## Disposition workflow

After a review, the author or maintainer dispositions each finding:

- `confirmed` — apply the smallest correction through the normal
  implementation and verification workflow;
- `rejected` — with evidence (why the finding does not apply);
- `unresolved` — remains an explicit decision, recorded as such.

Product trade-offs are explicit decisions, not defects. Dispositions are
recorded in the review record.

## Predeclared finite limits

| Limit | Default | Purpose |
| --- | --- | --- |
| Review pass | 1 | No automatic convergence loop, no retries. |
| Timeout | 300 s per review | A timed-out review is `timeout`, preserved. |
| Findings | 5 | Material findings cap; extras counted as truncated. |
| Output | 64 KiB per stream | Bounded capture; raw transcripts never enter records. |
| Reviewer invocations | 1 per review | Failures are recorded, not retried. |
| Paid model budget | 0 in this pilot | No real provider spend; stub fixtures only. |

## Adapter and invocation recipe

Implementation: `scripts/fresh-context-review.py` (+ package under
`scripts/fresh_context_review/`). It is an explicit opt-in manual command.
It is **never wired into sensors, hooks, or CI**; the only CI-wired entry
is the hermetic self-test suite (`scripts/tests/test_fresh_context_review.py`).

Assemble and launch a real review:

<!-- example: id=fresh_context_review_invocation classification=parser-check-only reason="Explicit opt-in adapter example; no model or network runs in documentation tests" -->
```bash
python3 scripts/fresh-context-review.py \
  --design-doc docs/designs/authz-delegation.md \
  --revision <frozen-commit> \
  --objective "Delegated authorization revocation timing" \
  --proposed-contract "revoke_delegation(...) invalidates cached grants" \
  --source-path crates/authz/src/delegation.rs \
  --test-path crates/authz/tests/delegation.rs \
  --doc-path docs/designs/authz-delegation.md \
  --reviewer-command-json '["/path/to/fresh-reviewer","--packet"]' \
  --parent-marker "MKR-7F3A" \
  --record-out .do-harness/reviews/authz-delegation.json
```

Notes:

- `--reviewer-command-json` is the explicit opt-in; without it or
  `--stub` the adapter records `not_run` and launches nothing.
- `--parent-marker` seeds the probe; the operator places the same marker in
  their own conversation only.
- `--limit-timeout`, `--limit-findings`, `--limit-output` override the
  predeclared defaults; `--meta` records host/client/model versions.
- Outcomes are recorded to `--record-out` (sanitized: fixed reason codes
  and structured findings; no raw transcripts). Packet assembly or adapter
  errors exit 2 without a record; all valid outcomes exit 0.
- Disposition: `python3 scripts/fresh-context-review.py --dispose RECORD
  --finding-id F1 --status confirmed --note "..."`.

## Smoke fixtures and predeclared expectations

Deterministic stub fixture mode (`--stub`): `scripts/fresh_context_review/
stub_reviewer.py` answers from committed expectation files. Expectations are
predeclared in `scripts/fresh_context_review/fixtures/smoke/`:

| Case | Scenario | Predeclared expectation |
| --- | --- | --- |
| `smoke-flawed` | `flawed` | `success_with_findings`, exactly F1+F2 (planted revocation-cache defect). |
| `smoke-sound` | `sound` | `success_zero_findings` (independently checked sound migration design). |
| `smoke-fail` | `fail` | `failure` (reviewer exit 17). |
| `smoke-timeout` | `timeout` | `timeout` (deadline enforced; stub sleeps). |
| `smoke-unavailable` | `unavailable` | `not_run` (reviewer declares capability unavailable). |
| `smoke-leak` | `leak` | `failure` with `marker_leaked: true` (parent-only marker appears in reviewer output). |

A missed planted defect is a recorded negative result; a successful run does
not prove general effectiveness.

The smoke suite runs `python3 scripts/fresh-context-review.py --smoke`, which
executes all six cases against the stub and writes the committed record
`plans/fresh-context-review-smoke-record.json`. It exits nonzero if any case
misses its predeclared expectation.

## Smoke-test record

Committed at `plans/fresh-context-review-smoke-record.json`. It contains per-
case outcome, expectation, finding ids, marker probe result, and elapsed
time, plus the suite-level usage note (zero model/token usage in stub mode).

## Observations and limitations

Filled in from the committed smoke record after the stub run (see the record
for exact numbers). Local stub smoke on this branch (commit
`f529824f0ea1c1946c0bc0a38e92c4dcd2ceaa57`, the snapshot that produced the
committed record) passed all six predeclared
design returned zero findings; fail/timeout/unavailable/leak paths produced
`failure`/`timeout`/`not_run`/`failure` (leak) as declared. Elapsed per case
is subsecond to a few seconds (the timeout case burns its 2 s budget).
Stub runs use no network, model, or tokens: currency cost and token usage
are recorded as unavailable.

Limitations:

- Stub smoke proves adapter mechanics **only**: frozen packets, fresh
  process launch, bounded capture, outcome classification, sanitized
  records. It does not prove model context isolation or defect-detection
  benefit — that requires a real reviewer run with the parent-only marker
  probe, recorded as the next opt-in step.
- A real reviewer run is opt-in and consumes no budget in this pilot. The
  next step is to run one recorded real pass (cheap local executor per the
  `docs/workflow-benchmarks.md` adapter contract, or a provider adapter with
  explicit budget) and record its findings, elapsed, and usage with
  provenance.
- Host/client/model versions are recorded only when supplied via `--meta`.

## Handoff to #499

Issue #499 owns repeated-effectiveness measurement and the adoption
decision. This pilot supplies: the packet/finding formats, the adapter, the
deterministic fixtures, the smoke record, and the isolation-evidence rules.
Issue #499 should reuse the paired-run protocol from `docs/workflow-benchmarks.md`
(identical inputs, alternating order, predeclared tolerance and sample size)
to measure whether a fresh-context reviewer reduces unresolved assumptions
before implementation, and then decide adoption. This record deliberately
states that comparative value and adoption remain undecided.
## Acceptance mapping

- [x] Eligibility rules keep the pilot optional and focused on consequential design choices.
- [x] Packet provenance identifies the exact design and source reviewed (pinned commit + blob SHAs + packet hash).
- [x] The reviewer receives no parent conversation; inherited host instructions and isolation-evidence limits are documented.
- [x] Success, zero findings, failure, timeout, and unverified isolation are distinct outcomes.
- [x] Findings meet the concrete-impact contract and receive evidence-backed dispositions.
- [x] The flawed and sound design smoke cases have recorded outcomes, including misses and false positives (predeclared expectations checked).
- [x] Runtime/output/usage limits and actual pilot cost are recorded (stub: zero usage; per-case elapsed in the smoke record).
- [x] Existing computational completion and merge requirements remain mandatory (unchanged: `verify`, evidence-freshness, PR review).
- [x] No full EGM installation, multi-stage swarm, default CI model run, or permanent workflow gate is introduced.
- [x] The experiment record states that comparative value and adoption remain undecided.
