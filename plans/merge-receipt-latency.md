# Merge receipt latency: expire on the validator's clock, not the generator's

> **Status:** active - #574 implemented on `fix/merge-receipt-latency-20261116`; evidence pending
> **Issue:** #574
> **Target:** `main` at `d7d9d0d` (v0.4.7 plus #573); line references are from that commit
> **Delivery:** sequential PRs merged with `pr-triage`, libSQL dogfood evidence, target release **0.5.0**
> **Recorded:** `plans/`

## 1. Symptom

On 2026-11-16, `scripts/pr-triage/merge.sh` refused to merge PR #573 twice in a
row, although the PR was `CLEAN` with every check green:

    error: PR 573 merge authorization is indeterminate (1 reason(s)):
      [receipt_expired] receipt expired at unix 1794821796 (now 1794821817)
    merge: PR 573 cannot be judged (unknown state or rejected receipt); not merging

The receipt was sealed at `issued_at = 1794821736` with a 60-second lifetime.
The validating call read the clock at `1794821817` — 81 seconds later. `pr ready`
itself measured 42-80 seconds per call on the same repository, so the failure is
deterministic on a slow host, not a race an operator can retry away. The only
way to land the PR was a hand-written `gh pr merge` after the same checks, which
reproduced the check-then-act window the receipt exists to close.

## 2. What the code does today (correcting the issue's diagnosis)

The issue proposes stamping `issued_at` at snapshot completion as fix (1). That
is already the behaviour:

- `readiness.rs:223` reads `now_unix` after steps 1-7 (view, checks,
  conversations, findings, policy, ancestry, re-read of mutable refs).
- `receipt.rs:159-160` seals `issued_at = now` and `expires = issued + 60`.

The real mechanism is on the **consumer** side. `pr ready --expected-receipt`
recomputes the whole snapshot, then calls `apply_expected_receipt` with a clock
read at `readiness.rs:223` of that second call. So the comparison is:

    now = issued_at + (validator collection time) + (queueing between calls)

With `validator collection ≈ 81 s > 60 s`, the receipt is expired before the
validator can finish, no matter how quickly it started. The 60-second TTL
silently assumes validation is instantaneous; it costs the same as generation.

Consequence for the plan: fix (1) alone is a no-op. Retrying cannot help.
The slice must change either the clock used for the expiry comparison, the TTL
budget, or both.

## 3. Decisions

- **D1 - Expiry is evaluated when validation begins.** Capture
  `validation_started_unix` once at the top of `evaluate_with_receipt` (after
  reading the supplied document) and pass it to `apply_expected_receipt` for the
  expiry check. Keep the post-collection snapshot-equality check unchanged:
  expiry answers "was this receipt stale when we started?", equality answers
  "did anything move while we validated?". This is the semantic fix.
- **D2 - TTL is bounded by generation cost.** Record `generation_seconds` in
  the receipt (start of the successful collection attempt to `issued_at`, clamped
  at `>= 0`), and seal `expires = issued + max(60, 2 * generation)`. A 120 s
  collection yields a 240 s receipt, so an immediate revalidation of comparable
  cost keeps at least the documented 60 s of headroom.
- **D3 - Hard cap.** Add `MAX_RECEIPT_LIFETIME_SECONDS = 600`. The validator
  accepts a supplied lifetime in `60..=600` and rejects anything outside it as
  `receipt_invalid`. The cap keeps the reuse window honest instead of widening
  to whatever the API did; do not make it configurable. The sealed lifetime must
  also equal `max(60, 2 * generation_seconds)` for the recorded generation: a
  hand-edited expiry cannot claim a long collection it did not pay for.
- **D4 - Schema version 2.** `generation_seconds` is a new required field (no
  `serde(default)`) and the accepted lifetime range changed, so old receipts must
  fail with a clear version mismatch, not a confusing lifetime error. Update all
  fixtures.
- **D5 - Diagnostic explains the failure.** `[receipt_expired]` names
  `issued_at`, `generation_seconds`, `expires_at`, and `now`, plus the delta.
  Invalid-lifetime messages name the accepted range.
- **D6 - `merge.sh` keeps its exit contract.** Exit 2 stays "cannot be judged",
  but an expiry is reported as a latency/usage condition: regenerate the report
  (optionally after a retry) rather than implying an unreadable API. No new exit
  code; the skill's documented contract stays 0/1/2/3.
- **D7 - No `--receipt-ttl` flag.** The issue's option 3 is unnecessary once D1
  and D2 land, and it would add a CLI/config surface plus docs and completion
  churn for a bounded internal budget. Record why it is not adopted.

## 4. Implementation

### Step 0 - Baseline

- `do-harness sync --switch` (checkout is clean, 9 behind `origin/main`).
- Dogfood check (finding, resolved): the checkout redirects `CARGO_TARGET_DIR`
  to `/tmp/do-harness-target`, so `target/release/do-harness` was a stale 0.4.6
  build while the workspace was 0.4.7. Resolve the real target directory,
  rebuild there, then run `doctor` and `init-db --check`; a stale binary is a
  hard stop for verify, hooks, and the merge sweep, not background noise.
- Export `DO_HARNESS_BIN=/tmp/do-harness-target/release/do-harness` for hooks,
  `pr-triage` and the merge sweep.
- Branch: `fix/merge-receipt-latency-20261116`.

### Step 1 - Make room under the ceiling first

`crates/do-harness/src/pr/readiness/receipt.rs` is 482 lines today (issue #566).
Run `target/release/do-harness split crates/do-harness/src/pr/readiness/receipt.rs`
to extract the inline `#[cfg(test)] mod tests` into `receipt_tests.rs` before
adding the new fields and assertions. This is the documented move-only route and
keeps the slice out of a forced mid-feature split.

### Step 2 - `reason.rs`: contract constants and field

- Keep `RECEIPT_LIFETIME_SECONDS = 60` as the **minimum**; rename the doc comment
  to say so.
- Add `MAX_RECEIPT_LIFETIME_SECONDS: i64 = 600`.
- Add `generation_seconds: i64` to `MergeReceipt` with a doc comment tying it to
  the seal calculation.
- Bump `RECEIPT_SCHEMA_VERSION` to `2`.

### Step 3 - `readiness.rs`: measure generation, validate on entry

- At the top of `evaluate_with_receipt`, capture `validation_started_unix`.
- At the top of the attempt loop, capture `attempt_started_unix`; when sealing,
  compute `generation_seconds = now - attempt_started` (saturating, `>= 0`).
- Add `generation_seconds` to `SnapshotInputs` and pass it to `authorize`.
- Use `validation_started_unix` for `apply_expected_receipt`; use the sealing
  clock unchanged for `build_receipt`.
- Keep the `MAX_SNAPSHOT_ATTEMPTS` retry semantics: only the successful
  attempt's duration is recorded, so a retry storm does not inflate the TTL.

### Step 4 - `receipt.rs`: seal and validate

- `build_receipt`: `expires = issued + max(60, 2 * generation_seconds)`; write
  `generation_seconds`.
- `validate_receipt`: require schema 2 and PR match; require lifetime within
  `60..=MAX` and equal to the generation-derived lifetime; keep the
  future-timestamp guard; compare expiry against the supplied `now`; keep
  snapshot equality last.
- Expired diagnostic: include issued, generation, expires, now, and the age.
- Invalid-lifetime diagnostic: name the accepted range.

### Step 5 - `merge.sh`

Change only the exit-2 message for the expired case so it states the cause
("receipt expired during revalidation") and the action ("regenerate the report
and retry; the receipt is now generation-bounded"). Keep the script free of new
flags and dynamic execution; the `untrusted-ingest` sensor must stay green.

### Step 6 - Tests

Unit (`receipt_tests.rs`, after the split):

- fast generation (0-2 s) seals a 60 s lifetime;
- 120 s generation seals 240 s and validates at `issued + 200`;
- 120 s generation validates at `issued + 241` as `Expired`;
- lifetime 61 with a 10 s generation is still accepted (floor is a floor, not an
  equality);
- lifetime 45 and lifetime 601 are `Invalid` with the range in the message;
- schema 1 is `Invalid` with a version mismatch;
- negative/backwards clock clamps generation to 0.

Integration (`tests/pr_authorization.rs`):

- update the exact-60 assertion at lines 46-49 to assert the floor for a fast
  fixture and the cap boundary;
- keep the 3600 s forged lifetime as `receipt_invalid`;
- add: rewrite a generated report's `expires_at` to `issued + 240` with
  `generation_seconds: 120` and assert `MERGE_ALLOWED` (proves the validator
  accepts the widened bounded window without a slow real run);
- add: an expired document keeps exit 2 and `receipt_expired`, and the stderr
  names issued/generation/expires/now;
- keep the moved-snapshot test unchanged (equality must still win).

The issue's "artificially slow collection" fixture is covered by injecting
`generation_seconds` into `build_receipt` and by the rewritten-window
integration case; no `sleep`-based test in the suite.

### Step 7 - Fixtures and plan record

- `tests/fixtures` and `.agents/skills/pr-triage/evals/walkthrough.sh:220`
  receipt fixture: `schema_version: 2`, add `generation_seconds: 0`.
- Move this document to `plans/merge-receipt-latency.md` when work starts, set
  `Status: active`, and add it to the Live records table in `plans/README.md`.
  On merge, set `Status: recorded` and note the shipped schema version.
- No `plans/invariants.json` entry is needed: this is a contract correction, not
  a new architecture rule.

### Step 8 - Evidence

- `cargo nextest run -p do-harness --test pr_authorization`
- `cargo nextest run -p do-harness pr::readiness` (unit matrix)
- `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --check`
- `do-harness verify --set verification --strict`
- `do-harness status --set verification` green
- `bash scripts/check-loc.sh` headroom for `receipt.rs`, `receipt_tests.rs`,
  `reason.rs`, `readiness.rs`, and `pr_authorization.rs`
- `python3 scripts/docs.py sync --check`, `examples --check`, `links --check`
  (the plans file is maintained; avoid unmarked CLI fences)
- `bash scripts/check-yamllint.sh` is not needed unless a workflow changes;
  `shell` covers `merge.sh`

### Step 9 - Merge workflow

This slice fixes the path it must use to land. Build `target/release/do-harness`
with the fix, export `DO_HARNESS_BIN`, generate the report with the new binary,
then run the normal `pr-triage` sweep. Do not use the manual `gh pr merge`
bypass; the report must revalidate under the new schema. If the installed
release binary is still v0.4.7, the pointer above is the only correct choice.

Commit subject (lowercase): `fix(pr): bound merge receipts by generation time (#574)`

## 5. Acceptance mapping

| Issue criterion | Covered by |
| --- | --- |
| Slow fixture still valid at revalidation | D2 + unit matrix; rewritten-window integration |
| 120 s collection retains >= 60 s | `max(60, 2 * generation)` unit case |
| Expired diagnostic names issued/generation/expires/now | D5 + stderr assertion |
| Regression: expired exits 2, slow path reaches `MERGE_ALLOWED` | Step 6 integration cases |
| Determinism on a slow repository | D1 removes validator latency from the comparison |

## 6. Risks and mitigations

- **Clock skew backwards.** Saturating subtraction; generation clamped to 0;
  future-timestamp guard unchanged.
- **Widened reuse window.** Hard cap at 600 s, validator range check, equality
  check still required. A forged `expires_at` cannot outlive the validator's own
  expiry comparison.
- **Cross-version receipts.** Schema bump fails closed with an explicit message;
  regenerating a report is one command.
- **Sweep path.** `pr sweep` writes a receipt then revalidates immediately
  (`sweep/machine.rs:316`); D1/D2 only make that path more reliable, and the same
  tests exercise it indirectly.
- **File ceiling.** The test split is a prerequisite, not a follow-up; if the
  source half still exceeds 450 lines after Step 4, decompose validation into a
  submodule before adding more.
- **Fixture drift.** `walkthrough.sh` receipt and `pr_authorization.rs` both pin
  schema and lifetime; update them together and let `verify` catch the rest.

## 7. Issue intake gate (roast, impact, close)

No issue is implemented on trust. Before a slot is taken, each open issue is
roasted against the current tree:

1. **Re-derive the claim.** Read the cited code and measure it on the synced
   `main`; the batch was written from a read-only analysis of `b3e5b69`, so line
   numbers and "current" behaviour drift (this plan already corrects #574's
   diagnosis).
2. **Check landed work.** Search `git log`, merged PRs, and existing sensors for
   the same fix. A duplicate or already-fixed issue is closed with the commit or
   PR that did it.
3. **Judge impact.** A verdict of `impact-high`, `impact-medium`, `impact-low`,
   `needs-spike`, `no-impact`, `duplicate`, or `already-fixed`, with the evidence
   (measurement, failing fixture, cost figure) in the issue.
4. **Close if there is no impact.** `gh issue close <n> --comment "<verdict,
   evidence, replacement link>"`. Closing is a recorded decision, never a way to
   skip inconvenient work; a low-impact item stays open and is ordered after
   0.5.0.
5. **Record the verdict.** `do-harness trace add` for every close, and one line
   in `plans/` when the verdict changes the 0.5.0 scope.

This gate runs for every issue in the delivery order, including the three
experiments (#498, #499, #458) and #568, before their slot is scheduled. The
intake roast for #574 is section 2 of this document.

## 8. Source grounding

Every external claim (tool behaviour, format, platform limit) is fetched and
quoted from the current official document at implementation time, with the
version or page date recorded in the issue/PR. Search snippets are pointers, not
evidence; the fetched page is.

Confirmed for the first two waves (fetched 2026-11-16):

| Topic | Official source | What it establishes |
| --- | --- | --- |
| Worktree discovery and safety | <https://git-scm.com/docs/git-worktree> | `remove` refuses unclean worktrees; `list --porcelain` reports `locked`/`prunable` (with reasons); `-z` is recommended for parsing |
| Build cache layout | <https://doc.rust-lang.org/cargo/reference/build-cache.html> | `target`/`build-dir` layout is internal and "subject to change"; `CARGO_TARGET_DIR` relocates it; sccache is the documented shared-cache route |
| CI base refs | <https://docs.github.com/en/actions/reference/workflows-and-actions/variables> | `GITHUB_BASE_REF`/`GITHUB_HEAD_REF` are set only for `pull_request`/`pull_request_target`, which is exactly why `--since @ci` must fail closed |

Fetch before coding, per issue: npm scoped-package and name-screening rules
(#565), SARIF 2.1.0 and GitHub code-scanning ingestion (#564), JUnit XML schema
(#564), PyPA packaging and Go toolchain docs (#563), cargo target-dir locking
(#571), Codecov upload/coverage semantics (#567). Record each link in the PR
body; a claim without a fetched source is not ready to implement.

## 9. Delivery contract for 0.5.0

- **Sequential, no stacking.** One issue per branch and PR; `pr-triage` reviews,
  roasts, resolves every thread, waits for every check, and merges with
  `--match-head-commit` before the next branch starts. Main stays green between
  merges.
- **Order.**
  0. Sync, rebuild, `doctor`/`init-db --check`, dogfood baseline.
  1. Intake gate over all open issues; close the no-impact ones.
  2. #574 (this document).
  3. #570 -> #571 -> #569 -> #572 (cleanup chain; #569 depends on both).
  4. #559, #560 (small docs correctness; both have can-fail tests).
  5. #561, #567 (decisions; adopt only with measured cost).
  6. #562, #563, #564 (features; #564 may split JUnit then SARIF).
  7. #566 (move-only decomposition, opportunistic; #574 already extracts
     `receipt.rs` tests).
  8. #565 (spike, then implement only if the spike passes).
  9. #568 (dated checkpoints; no code).
  10. #498, #499, #458 (intake gate: adopt with a date or close with a verdict).
- **DB dogfood, every slice.** The libSQL store is the source of truth:
  `task add` with the matching `plans/methods.json` method;
  `task advance`/`done` only on named sensor-ok beats; `verify --record` scoped
  to the branch; `trace add` for every decision and close; `init-db --check`
  before and after migrations; `task export` at the end of the wave; `metrics`
  and `dora` reviewed after the release candidate. `.do-harness/` is never
  committed.
- **Findings bar.** Every run is `--strict`: no warnings, no skips, no vacuous
  zero-check sensors. Real findings are fixed in the same PR; a false positive is
  narrowed in the sensor with a test, never blessed upward. `--bless` may only
  lower a baseline. Waivers go through `plans/invariants.json` with a reason.
  A finding that cannot be fixed in-slice becomes a linked issue before merge.
- **Release 0.5.0.** Once the feature wave is merged and main is green,
  `scripts/release-manager.sh next` must derive `0.5.0` from the `feat` commits.
  If it does not, stop and investigate instead of forcing a version. On a
  derived 0.5.0, `prepare 0.5.0` in a release PR (manifests, lock, npm, man),
  run `release-manager-e2e` and `publish-npm`, merge through `pr-triage`, and
  confirm the tag, the GitHub Release, crates.io, and npm only after
  `release.yml` completes. A tag without a Release is not a release.

## 10. Out of scope

- Removing the receipt boundary or merging on a bare `pr ready`.
- A `--receipt-ttl` flag or any user-tunable TTL (D7).
- Changing `merge.sh` exit codes or the skill's merge policy.
- The wider `pr` module refactors tracked by #566 beyond the test split needed
  here.
- Any issue judged `no-impact` at the intake gate; it is closed with evidence,
  not implemented.
