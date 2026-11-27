# Fresh-context review: opt-in pilot experiment (issue #498)

Status: pilot complete — the deterministic suite passes continuously and one
recorded live pass per fixture case exists below; effectiveness and
adoption remain undecided (issue #499 owns that verdict).

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

Implementation: the native `do-harness design-review` command
(`crates/do-harness/src/design_review/`). It is an explicit opt-in manual
command usable in any git repository; it is **never wired into sensors,
hooks, or CI**, and the only CI-wired entry is the hermetic integration
suite (`crates/do-harness/tests/design_review.rs` and
`design_review_bridge.rs`, the `fresh-context-review` sensor). The pilot
first landed as repository-local Python (`scripts/fresh-context-review.py`);
it moved into the CLI so every adopting codebase can run design reviews,
per the Repository-Agnostic Capability Contract in AGENTS.md — a capability
that only works in this checkout is a defect, not a feature.

Assemble and launch a real review:

<!-- example: id=fresh_context_review_invocation classification=parser-check-only reason="Explicit opt-in native command example; no model or network runs in documentation tests" -->
```bash
do-harness design-review run \
  --design-doc docs/designs/authz-delegation.md \
  --revision <frozen-commit> \
  --review-id issue-498-authz \
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

- `--reviewer-command-json` is the explicit opt-in; without it the command
  records `not_run` and launches nothing.
- `--parent-marker` seeds the probe; the operator places the same marker in
  their own conversation only.
- `--limit-timeout`, `--limit-findings`, `--limit-output` override the
  predeclared defaults; `--meta` records host/client/model provenance.
- Outcomes are recorded to `--record-out` (sanitized: fixed reason codes
  and structured findings; no raw transcripts). Packet assembly errors
  (unresolvable revision, pinned path missing at that revision, invalid
  JSON arguments) exit 2 without a record; every recorded outcome —
  success, failure, timeout, `not_run`, `unverified` — exits 0, because
  the record carries the verdict.
- Disposition: `do-harness design-review dispose --record RECORD
  --finding-id F1 --status confirmed --note "..."`.

### Host file-handoff bridge

`do-harness design-review bridge` adapts a host-side reviewer — an
operator-driven fresh agent session — to the reviewer process contract. It
receives the packet on stdin, writes it verbatim to
`<handoff-dir>/packet.json`, and waits for the host to place
`<handoff-dir>/findings.json`, relaying that file verbatim on stdout. It
waits out partial writes: the file is accepted once it parses as JSON, and
a response that never parses is relayed at the deadline so the adapter
classifies the invalid reviewer output as a failure. No findings file
within `--max-wait-s` exits 9 with `capability unavailable` on stderr — the
adapter's `not_run` contract, never a success. The bridge launches no model
and echoes neither the packet nor any host context; its hermetic tests live
in `crates/do-harness/tests/design_review_bridge.rs` (the
`fresh-context-review` sensor).

## Deterministic fixtures and predeclared expectations

The frozen fixture designs and their predeclared expectations are committed
at `crates/do-harness/tests/fixtures/design-review/`:

| File | Predeclared expectation |
| --- | --- |
| `flawed-design.md` + `flawed-expected.json` | The planted revocation-cache defect yields exactly findings F1+F2. |
| `sound-design.md` + `sound-expected.json` | The independently checked sound migration design yields zero findings. |
| `leak-marker.txt` | The parent-only marker appearing in reviewer output is `failure` with `marker_leaked: true`. |

A missed planted defect is a recorded negative result; a successful run does
not prove general effectiveness.

The deterministic mechanical proof is the hermetic integration suite
(`cargo test -p do-harness --test design_review --test
design_review_bridge`, the `fresh-context-review` sensor): the reviewer
process is the CLI's own bridge action with pre-placed findings files, so
every outcome class — success with findings, zero findings, failure
(nonzero exit, invalid JSON, missing fields, oversized output), timeout,
`not_run`, marker leak, `unverified` — is exercised in CI without ever
launching a model. The retired Python stub smoke suite produced the
committed stage-1 record below.

## Smoke-test record

Committed at `plans/fresh-context-review-smoke-record.json` (stage 1, the
Python stub suite before the capability moved into the CLI). It contains
per-case outcome, expectation, finding ids, marker probe result, and
elapsed time, plus the suite-level usage note (zero model/token usage in
stub mode). Its outcomes are now pinned continuously by the native
integration suite instead of a record-regenerating smoke command.

## Live reviewer pass (predeclared protocol)

One recorded real reviewer pass per case, run through the adapter with the
host file-handoff bridge. The protocol below was predeclared and committed
before the runs; the recorded outcomes follow it.

- **Reviewer**: one fresh-context child agent of the operating host session
  per case, launched with its own role instructions and a first message
  containing only the handoff paths. It inherits no parent conversation.
- **Cases**: `live-flawed` and `live-sound`, the same frozen fixture designs
  as the deterministic suite. The packet lists only the design document; the
  committed `*-expected.json` files stay operator-side because they contain
  the planted defect and expected findings (handing them to the reviewer
  would leak the answer).
- **Limits**: one review pass per case, no retries or convergence loop;
  adapter timeout 600 s; bridge `--max-wait-s 570`; at most 5 findings;
  64 KiB output cap.
- **Marker probe**: a unique parent-only marker placed in the operator's
  conversation only. Its value appears nowhere committed and is not given to
  the reviewer; the committed recipe keeps a placeholder. Expected:
  `marker_leaked: false`.
- **Predeclared expectations**: `live-flawed` → `success_with_findings` with
  at least one finding identifying the revocation/snapshot-invalidation
  defect class (the planted defect); a miss is a recorded negative result.
  `live-sound` → `success_zero_findings`; any finding is a recorded false
  positive, dispositioned individually.
- **Usage provenance**: the host does not expose a per-reviewer token meter;
  usage is recorded as `unavailable` with that reason, never estimated and
  never fabricated from elapsed time.
- **Isolation evidence**: parent-conversation isolation is evidenced only by
  the marker probe (a negative result per case). The child inherits
  host-injected instructions (its system prompt, the repository's AGENTS.md,
  and the skills catalog) that cannot be excluded, and it has host tool
  access — this host offers no read-only reviewer capability, so the
  instructions confine it to the packet and the documents it lists. Both
  facts are recorded as limitations, not proof.
- **Decision rule**: each finding is dispositioned `confirmed` / `rejected`
  with evidence / `unresolved`. Both fixture designs are synthetic, so
  dispositions adjudicate detection correctness only; no production
  correction follows from either case.

Invocation recipe (recorded; the same shape produced both committed live
records, with `<pinned-commit>` resolved to the predeclared-protocol
commit):

<!-- example: id=fresh_context_review_live_invocation classification=parser-check-only reason="Explicit opt-in live-run example; no model or network runs in documentation tests" -->
```bash
HANDOFF=".do-harness/fresh-review-handoff-live-flawed"
rm -rf "$HANDOFF" && mkdir -p "$HANDOFF"
do-harness design-review run \
  --design-doc crates/do-harness/tests/fixtures/design-review/flawed-design.md \
  --revision <pinned-commit> \
  --review-id live-flawed \
  --record-out plans/fresh-context-review-live-flawed.json \
  --objective "Delegated admin permissions apply transitively through group membership and must be revocable immediately." \
  --intended-behavior "grant_delegation grants perms to a group and its transitive members; revoke_delegation withdraws them transitively; effective_permissions reflects both at authorization time." \
  --constraint "revocation must be immediate, never stale" \
  --constraint "cache use is allowed but must not serve stale data" \
  --constraint "no new schema in scope" \
  --proposed-contract "grant_delegation(delegator, group_id, perms); revoke_delegation(delegator, group_id, perms); effective_permissions(subject_id) -> set[perm]" \
  --open-question "Is eventual consistency acceptable for revocation?" \
  --open-question "Is the delegation snapshot invalidated on every graph mutation?" \
  --doc-path crates/do-harness/tests/fixtures/design-review/flawed-design.md \
  --reviewer-command-json "[\"do-harness\",\"design-review\",\"bridge\",\"--handoff-dir\",\"$HANDOFF\",\"--max-wait-s\",\"570\"]" \
  --parent-marker "<unique-parent-only-marker>" \
  --limit-timeout 600 \
  --meta '{"reviewer_provenance":"operator-supplied fresh-context child agent"}'
```

The `live-sound` case is identical in shape with
`sound-design.md`/`live-sound` names, the sound objective and contract, and
its own handoff directory. The recorded packets pin every input; the
committed records are `plans/fresh-context-review-live-flawed.json` and
`plans/fresh-context-review-live-sound.json`.

### Recorded live outcomes

Run at commit `fe9c100` (the predeclared-protocol commit), reviewer = one
fresh-context child agent of the operating host session per case (no parent
conversation), through the native command with its own bridge as the
reviewer process:

| Case | Outcome | Findings | Marker probe | Elapsed |
| --- | --- | --- | --- | --- |
| `live-flawed` | `success_with_findings` | 5, not truncated: F1 is exactly the planted revocation-cache defect (revoke never invalidates `delegation_snapshot`); F2–F5 are further concrete defects of the same frozen design (membership-mutation invalidation, provenance-free snapshot union, missing delegator authorization precondition, no-new-schema contradiction) | `false` (clean) | 199.5 s |
| `live-sound` | `success_zero_findings` | 0 — no false positives | `false` (clean) | 43.0 s |

Both cases met their predeclared expectations. Dispositions: all five
`live-flawed` findings are `confirmed` (detection adjudication only — the
design is synthetic, so no production correction follows); the sound case
has no findings to disposition. Usage provenance: the host exposes no
per-reviewer token meter, so token/currency usage is recorded as
`unavailable` for that reason — elapsed time is never converted into a
usage claim. The marker value appears nowhere committed; the records carry
only the probe booleans.

## Observations and limitations

Stage 1 (the retired Python stub suite, commit
`f529824f0ea1c1946c0bc0a38e92c4dcd2ceaa57`): all six predeclared stub
expectations passed — the flawed design produced F1+F2, the sound design
returned zero findings, and the fail/timeout/unavailable/leak paths produced
`failure`/`timeout`/`not_run`/`failure` (leak) as declared; per-case elapsed
is subsecond to a few seconds, with zero model/token usage. Those outcome
classes are now pinned continuously by the native integration suite.

Stage 2 (the recorded live pass, above): both fixture cases met their
predeclared expectations on the first and only pass — the planted defect
was detected with four further concrete findings, and the sound design
produced zero false positives. Marker probes were negative in both cases,
so no parent-conversation content reached either reviewer.

Limitations:

- One pass per case, one reviewer (the operating session's own fresh
  child agents), two synthetic fixture designs, a single host: this is
  feasibility evidence only. It does **not** establish comparative
  defect-detection benefit or adoption value; issue #499 owns that
  measurement and verdict.
- The deterministic suite proves command mechanics **only**: frozen
  packets, fresh process launch, scrubbed environment, bounded capture,
  outcome classification, sanitized records.
- Isolation evidence is the negative marker probe per case, plus the
  native launch record (fresh process, scrubbed environment key list).
  The reviewer child inherits host-injected instructions (its system
  prompt, the repository's AGENTS.md, and the skills catalog) that cannot
  be excluded, and it has host tool access — this host offers no
  read-only reviewer capability, so the instructions confine it to the
  packet and the documents it lists. Both facts are limitations, not
  proof.
- Usage provenance: the host exposes no per-reviewer token meter, so
  usage is recorded as `unavailable` with that reason; host/client/model
  identity is recorded only when supplied via `--meta`.

## Handoff to #499

Issue #499 owns repeated-effectiveness measurement and the adoption
decision. This pilot supplies: the packet/finding formats, the native
`do-harness design-review` command, the deterministic fixtures and
integration suite, the stage-1 smoke record, the recorded live outcomes,
and the isolation-evidence rules.
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
- [x] The flawed and sound design smoke cases have recorded outcomes, including misses and false positives (predeclared expectations checked deterministically; the live pass recorded both cases matching their predeclared expectations — defect detected, zero false positives).
- [x] Runtime/output/usage limits and actual pilot cost are recorded (stage 1: zero stub usage; live pass: predeclared 600 s/5/64 KiB limits, 199.5 s and 43.0 s elapsed, usage `unavailable` because the host exposes no per-reviewer token meter).
- [x] Existing computational completion and merge requirements remain mandatory (unchanged: `verify`, evidence-freshness, PR review).
- [x] No full EGM installation, multi-stage swarm, default CI model run, or permanent workflow gate is introduced.
- [x] The experiment record states that comparative value and adoption remain undecided.
