# Epic: pr-triage Agent Skill

> **Status:** phases 1–5 complete (skill + evals; `pr no-effect` + `pr review` with cache; proof skipping + `false_proven`; measurement + benchmark; semantic routing cost/safety benchmark with a recorded `no-go` verdict)
> **Related:** Agent Skills open standard, GitHub PR lifecycle, optional `do-harness` token reduction
> **Created:** 2026-09-11

## Why

Maintaining many open PRs in a personal GitHub account means repetitive work:
checking impact, updating branches, waiting on checks, answer review comments,
roasting the change, fixing findings, and merging in order. GitHub features that
would offload this (merge queue, reliable auto-merge with up-to-date branches)
are unavailable or unreliable on personal accounts.

This epic ships the workflow as one portable Agent Skill so any already-running
coding CLI (opencode, Claude Code, pi, and other skills-compatible agents) can
triage all open PRs end to end. `do-harness` is an optional dependency: when
present it reduces review tokens by emitting only the unresolved semantic
residual plus evidence; the skill works with `gh` + `git` alone.

## Slices

| Slice | Exit criteria | Evidence |
|-------|---------------|----------|
| Phase 1: skill | `.agents/skills/pr-triage/` with `SKILL.md`, references, deterministic scripts, Claude symlink; passes `quick_validate.py` and `skills` sensor; end-to-end run over real PRs with no wrong merges | skill files + manual sweep report |
| Phase 2: do-harness commands | `do-harness pr no-effect` and `do-harness pr review` (residual + minimal context + skipped-unit audit) in universal mode (git-fallback root, no `do-harness.toml` required); ATDD coverage | `cargo test`, fixtures |
| Phase 3: proof skipping | evidence mapping marks mechanical units proven and lists them for audit; behavioral units always residual; `false_proven` oracle | seeded-defect fixtures |
| Phase 4: measurement | context-inclusive baseline (`T_raw`, `T_res`) recorded per sweep; no-go if residual >= raw on ordinary PRs | benchmark report |
| Phase 5: semantic routing | optional typed router selects review depth; all error/uncertain states fail toward depth; end-to-end cost + seeded-route oracle over 12 classes; regression gate in `cargo test` | `scripts/pr-routing-benchmark.sh` + `tests/pr_routing.rs`; recorded `no-go` verdict |

## Phase 1 requirements (roast findings)

- **Stacks:** detect `baseRefName != default`; merge parent first, then rebase
  the child onto the new base and force-push before validating it (squash breaks
  stacks otherwise).
- **Idempotency:** state at `.git/pr-triage/state.tsv` keyed on
  `(pr, head_sha, base_sha)`; unchanged + green PRs are skipped on repeat sweeps.
- **Check binding:** classify checks and commit statuses against the current
  `headRefOid`; never trust stale green checks from a previous commit.
- **Conversations:** review threads are fixed/replied then resolved via
  `resolveReviewThread`; top-level issue comments are replied to (no resolve
  state exists).
- **Post-merge verification:** verify default-branch runs for the merge SHA;
  open and merge a revert PR and halt the sweep on failure.
- **Author policy:** auto-triage own + bot PRs only; other authors get findings
  as comments, never branch updates, thread resolution, or merges.
- **Injection boundary:** command allowlist; never execute PR code, scripts,
  builds, or installs; fixes are edits + push only.
- **Machine fast path:** known bot + lockfile-only + green + no threads merges
  with zero LLM review.
- **No auto-merge:** direct `gh pr merge --squash --match-head-commit` only.

## Boundaries

- **Skill owns:** PR lifecycle orchestration, ordering, comment handling,
  review/roast instructions, state, escalation.
- **`gh`/`git` own:** GitHub control plane and repository truth.
- **do-harness owns (optional):** no-effect decision, semantic residual,
  evidence-skipped units, minimal context.
- **Agent runtime owns:** model, tools, permissions, sandboxing, session length.

## Evidence

- Skill structure: `python3 .agents/skills/skill-creator/scripts/quick_validate.py .agents/skills/pr-triage`.
- Evals: `do-harness eval --skill pr-triage` runs a hermetic walkthrough (fake
  `gh`, local git repo) and grades 24 assertions across 5 cases; blessed with a
  0.95 bar floor and pinned grader SHA-256 baselines.
- Shell scripts: `shellcheck` clean.
- Manual run: one sweep over open PRs; record per-PR decision and any escalation.
- Phase 2+: `cargo test`, `do-harness verify`.

## Non-goals

- Workflows, drivers, daemons, auto-merge.
- Forges other than GitHub in v1.
- LLM SDKs or model selection inside `do-harness`.
- Numeric risk scores, full PR state mirror.

## Phase 2 completion — `pr no-effect` + `pr review` (2026-09-11)

- `do-harness pr no-effect` — merge-base tree delta with local-first, `gh`
  compare fallback; ATDD in `crates/do-harness/tests/pr_no_effect.rs`.
- `do-harness pr review` — deterministic residual: one unit per changed hunk
  with git's function-context anchor (`crates/do-harness/src/pr/diff.rs`),
  merge-base policy probe and report assembly (`crates/do-harness/src/pr/review.rs`),
  best-effort cache under the repository git directory with `--recompute`
  bypass (`crates/do-harness/src/pr/cache.rs`).
- Universal mode: git-work-tree root fallback (`pr::command::resolve_root`), no
  `do-harness.toml` and no initialization required. PR mode prefers local
  revisions and falls back to `gh pr diff` when the clone cannot resolve them.
- Security invariant: `.github/pr-gate.toml` is read with
  `git show <merge-base>:...`, never from the PR head; parse failures keep the
  units residual and surface as warnings. `skipped` and `false_proven` are
  frozen schema fields and stay empty until phase 3 proof mapping.
- Evidence: `tests/pr_review.rs` 7 cases (residual units, cache hit and
  invalidation, no-effect short-circuit, merge-base policy trust, malformed
  policy warning, missing-rev exit 1, usage exit 2); `pr::diff` 10 parser
  tests; `cargo test --workspace`, `cargo clippy --workspace --all-targets`,
  and `do-harness verify` green on the slice.

## Phase 3 completion — proof skipping + `false_proven` (2026-09-11)

- Policy schema: `.github/pr-gate.toml` `[proof]` table with `mechanical` and
  `behavioral` glob arrays (`crates/do-harness/src/pr/proof.rs`), parsed with
  `deny_unknown_fields`; absent, malformed, or invalid-glob policy proves
  nothing.
- Verdicts: mechanically matched paths and structural rename-only/mode-only
  units move to `skipped` for audit; behavioral matches always stay residual;
  a mechanical claim contradicted by a behavioral rule (or the protected policy
  path) is revoked into `false_proven` and kept residual.
- Report schema bumped to v2; phase-2 cache entries are ignored, not migrated.
- Evidence: `pr::proof_tests` guard suite (6 cases) plus `tests/pr_review.rs`
  now 11 cases including the seeded-defect oracle (`mechanical = ["**/*.rs"]`
  overridden by `behavioral = ["crates/**"]` never skips the defect), mechanical
  lockfile audit, invalid-glob fail-closed, and structural rename proof.

## Phase 4 completion — context-inclusive measurement (2026-09-11)

- `pr review` report v3 adds `measurement`: `t_raw` (unified-diff bytes), `t_res`
  (serialized residual payload bytes), `ratio`, and `verdict` (`reduced` iff
  `t_res < t_raw`, otherwise `no-go`). Context shared by both review paths
  cancels out; the JSON envelope counts against the residual.
- `scripts/pr-review-benchmark.sh [COUNT]` replays the last first-parent commits
  and reports `t_raw`/`t_res`/`verdict`/`skipped` per commit plus ordinary-PR
  totals. Run:
  `DO_HARNESS_BIN=target/debug/do-harness bash scripts/pr-review-benchmark.sh 15`.
- Benchmark over this repository's 15 first-parent commits (no proof policy
  present, so all ordinary): **2 reduced / 13 no-go**. Finding: the JSON
  residual does not beat `gh pr diff` for ordinary PRs; the win comes only when
  proof skipping removes units (asserted in `tests/pr_review.rs` with a skipped
  lockfile). The skill now reviews the residual only on `reduced`, falls back to
  `gh pr diff` on `no-go`, and records the four measurement fields per sweep.
- Evidence: `review::tests` pins the reduced/no-go/empty computation;
  `tests/pr_review.rs` asserts measurement fields and the reduced case.

## Phase 5 completion — end-to-end routing cost and safety gate (2026-09-18)

- `scripts/pr-routing-benchmark.sh` measures three arms over a seeded 12-class corpus
  (`tests/fixtures/pr-routing/`): **A** raw unified diff, **B** the current `pr review`
  residual policy (the same `measurement.verdict` probe the skill uses, so B is not a
  strawman), and **C** semantic routing — the router judgment plus the review payload it
  selects. Deterministic fake provider (`fake-router.sh`), so a run is byte-identical.
  Byte fields are byte proxies for deterministic CI; they are never tokens.
- Run: `DO_HARNESS_BIN=target/debug/do-harness bash scripts/pr-routing-benchmark.sh --fixtures tests/fixtures/pr-routing`.
- Result over the 12 seeded classes:

  | metric | value |
  |---|---|
  | total model input (C) | 8484 B |
  | baseline model input (B) | 3253 B |
  | mean / median / p95 savings vs B | −1.705 / −1.678 / −1.579 |
  | reduced vs B / no-go vs B | 91.7% / 8.3% |
  | router calls per case | 1.00 |
  | seeded oracle failures / downgrades | 0 / 0 |
  | timeout-or-invalid fallbacks | 1 |

  `VERDICT no-go: total model input is not lower than Baseline B, or seeded escalations regressed`.
- **Safety is proven, economy is not.** The route oracle passes on all 12 classes with zero
  downgrades, including the adversarial `misleading` provider (its judgment looks `cheap` while
  asserting `public_contract_change: yes`) and the `malformed` provider — the six high-impact
  classes (`public-api-break`, `security`, `persistence-schema`, `concurrency`, `mixed-buried`,
  `router-invalid`) all stay `deep`. The routing *decision* is therefore safe to rely on.
- The economy verdict is negative on this corpus because each fixture diff is only ~150–800 B, so
  the router envelope (request + judgment, ~330–430 B each) is larger than the diff it is meant to
  save. Routing only pays when the potential review payload is substantially larger than the
  judgment envelope; the three largest classes (`mixed-buried`, `security`, `public-api-break`) are
  the only ones where the ratio improves, and none reach break-even.
- Regression gate: `crates/do-harness/tests/pr_routing.rs` runs the benchmark inside the existing
  `test` sensor — exit 0, every seeded class at or above its declared minimum route, `downgrades=0`,
  `seeded_oracle_failures=0`, and byte-identical output across two runs. No new sensor, no new
  `verify` cost.
- **Next action:** a `go` verdict requires the review payload to dominate the router envelope —
  i.e. a corpus of realistic large diffs (thousands of lines, the case where residual savings are
  hundreds of KB) rather than minimal fixtures, and ideally a single batched routing call per
  sweep instead of one per PR. Until then the router stays optional and experimental: no default
  enables it.

## Task tracking note

`plans/methods.json` only catalogs Rust-sensor methods (`vertical-event-slice`,
`spike-and-resolve`); phase 1 is markdown/shell with no applicable sensor, so it
is tracked by this epic rather than `do-harness task add`. Phase 2+ Rust work
uses the existing methods catalog.
