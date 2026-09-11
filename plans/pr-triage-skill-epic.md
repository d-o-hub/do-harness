# Epic: pr-triage Agent Skill

> **Status:** phase 1 complete (skill + evals); phase 2 complete (`pr no-effect` + `pr review` with cache)
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

## Task tracking note

`plans/methods.json` only catalogs Rust-sensor methods (`vertical-event-slice`,
`spike-and-resolve`); phase 1 is markdown/shell with no applicable sensor, so it
is tracked by this epic rather than `do-harness task add`. Phase 2+ Rust work
uses the existing methods catalog.
