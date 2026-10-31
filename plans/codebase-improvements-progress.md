# Post-v0.4.1 codebase improvements progress

## Baseline

- **Observation and update timestamp:** 2026-10-31T09:21:16Z
- **Current published release:** [do-harness v0.4.1](https://github.com/d-o-hub/do-harness/releases/tag/v0.4.1) (Release ID `429800183`, published `2026-10-31T08:01:24Z`, immutable release baseline).
- **Original local HEAD:** `b29d9383872541ea34a9f584b6d7f6b06e1ed871`
- **Advertised remote main SHA:** `fbd3f4fef682c9cbeebefb5a018c855e0ef6aadf` (queried via read-only remote discovery)
- **Fetched execution-base SHA:** `fbd3f4fef682c9cbeebefb5a018c855e0ef6aadf` (fetched into local git tracking ref `refs/remotes/origin/main` and fast-forward merged to local `main`)
- **Implementation branch:** `chore/latest-main-guardrails` (branched from `fbd3f4fef682c9cbeebefb5a018c855e0ef6aadf`)
- **Upstream comparison:** [b29d938...fbd3f4f](https://github.com/d-o-hub/do-harness/compare/b29d9383872541ea34a9f584b6d7f6b06e1ed871...fbd3f4fef682c9cbeebefb5a018c855e0ef6aadf) (exactly 1 commit: `chore(release): v0.4.1 (#406)`)
- **Distinction note:** Read-only remote discovery (`git ls-remote`) observes advertised remote refs without updating local refs or worktree state. Remote fetch (`git fetch`) updates remote-tracking references (`origin/main`). Local branch fast-forward updates local branch pointers. Release provenance (`v0.4.1`) reflects published release distribution, while git SHAs identify source commits.

## Progress

| ID | Slice | State | Changed paths / behavior | Evidence | Next action |
|---|---|---|---|---|---|
| CG-01 | Baseline refresh | verified | Fast-forwarded local `main` to `fbd3f4fef682c9cbeebefb5a018c855e0ef6aadf`; created branch `chore/latest-main-guardrails` | Clean working tree; `git fetch` updated `origin/main` from `b29d938` to `fbd3f4f`; `git rev-parse HEAD` returns `fbd3f4fef682c9cbeebefb5a018c855e0ef6aadf` | Advance to CG-02 audit tracking records |
| CG-02 | Current audit | verified | Created `plans/codebase-improvements-progress.md`; appended post-v0.4.1 review and FU-01–FU-03 follow-ups to `plans/analysis-followups.md` | Tracked files established and linked under `plans/` | Advance to CG-03 shared latest-main helper |
| CG-03 | Shared helper | verified | Created `.agents/skills/harness/scripts/latest-main.sh` with isolated Git environment, `--read-only`, and fetch modes | Unit tests in `test_latest_main.py` passed (7/7); tested `--read-only`, normal fetch, invalid args, non-git dir | Completed |
| CG-04 | Workflow preflights | verified | Updated `preflight.sh` in issue-to-pr & pr-triage; updated `start-item.sh` in issue-to-pr to call `latest-main.sh` and removed inline fetch | Hermetic workflow tests in `test_latest_main.py` passed (7/7); dry-run and error refusal verified | Completed |
| CG-05 | AGENTS/harness/skill instructions | verified | Updated `AGENTS.md`, `harness/SKILL.md`, `heuristics.md`, `htn-planner/SKILL.md`, `issue-to-pr/SKILL.md`, `pr-triage/SKILL.md` | `quick_validate.py` passed on all 4 skills; `check-skills-sync.sh` passed | Completed |
| CG-06 | Fixture gate/scaffolding/sensor | verified | Removed obsolete `reads_guidance` in `crates/do-harness/src/eval/fixture.rs`; registered template helper in `init/skills.rs`; added `test_latest_main.py` sensor and invariant | `fixture_tests` passed (3/3); `check-invariants` passed (72 headers); `check-skills-sync` passed; adopter smoke passed | Completed |
| CG-07 | End-to-end verification | blocked | Executed regression tests, binary build, adopter smoke, quick-validate, and full repository verification suite (40 sensors); eval `--strict-fixtures` blocked on `LIFT-MISS` | `test_latest_main.py` (7/7 ok); full no-set `verify` passed (40/40 sensors, exit 0); adopter smoke passed; `eval --strict-fixtures` reports `LIFT-MISS` against grandfathered local DB floors | Investigate deterministic fixture lift-floor policy resolution |

## Changes

### CG-01
- **Affected symbols / refs:** `refs/heads/main`, `refs/remotes/origin/main`, branch `chore/latest-main-guardrails`.
- **Previous behavior:** Local `main` was pinned at pre-v0.4.1 commit `b29d9383872541ea34a9f584b6d7f6b06e1ed871`.
- **New behavior:** Local `main` fast-forwarded to `fbd3f4fef682c9cbeebefb5a018c855e0ef6aadf` (v0.4.1 release commit), worktree clean on implementation branch `chore/latest-main-guardrails`.
- **Commit/PR:** not created.

### CG-02
- **Affected paths:** `plans/codebase-improvements-progress.md`, `plans/analysis-followups.md`.
- **Previous behavior:** No tracked progress file for post-v0.4.1 improvements; `analysis-followups.md` ended at v0.4.0 roadmap notes.
- **New behavior:** Documented baseline, progress table, and follow-ups FU-01 (migration consistency), FU-02 (locale direction audit ordering), FU-03 (missing advertised scheme matrix).
- **Commit/PR:** not created.

### CG-03
- **Affected paths:** `.agents/skills/harness/scripts/latest-main.sh`.
- **Previous behavior:** No shared latest-main helper; only an inline fetch existed inside issue-to-pr branch creation.
- **New behavior:** Standalone executable helper supporting `--root DIR` and `--read-only`, unsetting repository-selection variables, validating remote origin/main, and emitting SHA on stdout with diagnostics on stderr.
- **Commit/PR:** not created.

### CG-04
- **Affected paths:** `.agents/skills/issue-to-pr/scripts/preflight.sh`, `.agents/skills/pr-triage/scripts/preflight.sh`, `.agents/skills/issue-to-pr/scripts/start-item.sh`.
- **Previous behavior:** Preflights did not check origin/main; `start-item.sh` used inline fetch only when creating branches, leaving resume and dry-run stale.
- **New behavior:** Preflights and `start-item.sh` invoke `latest-main.sh` as first git step; dry-run passes `--read-only` and reports advertised-only status; new branches cut from fetched SHA.
- **Commit/PR:** not created.

### CG-05
- **Affected paths:** `AGENTS.md`, `.agents/skills/harness/SKILL.md`, `.agents/skills/harness/references/heuristics.md`, `.agents/skills/htn-planner/SKILL.md`, `.agents/skills/issue-to-pr/SKILL.md`, `.agents/skills/pr-triage/SKILL.md`.
- **Previous behavior:** No operational requirement to check `origin/main` as first step; skills assumed local HEAD or cached state was fresh.
- **New behavior:** Encoded "Latest-main First" as invariant and Phase 0 in `AGENTS.md`, operational procedures in `harness/SKILL.md` and `heuristics.md`, pre-planning requirement in `htn-planner/SKILL.md`, and preflight requirements with `--dry-run` forwarding in `issue-to-pr/SKILL.md` and `pr-triage/SKILL.md`.
- **Commit/PR:** not created.

### CG-06
- **Affected paths:** `crates/do-harness/src/eval/fixture.rs`, `crates/do-harness/src/eval/fixture_tests.rs`, `crates/do-harness/templates/skills/harness/scripts/latest-main.sh`, `crates/do-harness/src/init/skills.rs`, `do-harness.toml`, `plans/invariants.json`, `scripts/tests/test_latest_main.py`, `.agents/skills/harness/evals/**`, `.agents/skills/issue-to-pr/evals/**`.
- **Previous behavior:** `fixture.rs` forced copied-guidance assertions on `SKILL.md`; helper was not scaffolded by `init`; no automated sensor for latest-main rules.
- **New behavior:** `fixture.rs` removed the false `reads_guidance` requirement while retaining agent-output gates; `latest-main.sh` mirrored to templates and scaffolded by `init`; `latest-main-guardrails` sensor added to `do-harness.toml` and `plans/invariants.json`; walkthroughs updated to exercise real Git fixtures.
- **Commit/PR:** not created.

### CG-07
- **Affected paths:** Full verification surface.
- **Previous behavior:** No behavioral tests for remote advancement, read-only queries, or entrypoint preflights.
- **New behavior:** Hermetic behavioral regression matrix executed in `test_latest_main.py`; adopting workspace smoke verified embedded helper generation and bash execution; verification sensors confirmed green; eval `--strict-fixtures` flagged `LIFT-MISS` on harness and issue-to-pr due to raise-only floor mechanics against grandfathered local SQLite rows.
- **Commit/PR:** not created.

## Verification evidence

- `git --no-optional-locks status --porcelain`: cwd `/home/do/git/d-o-hub/do-harness`, rev `b29d9383872541ea34a9f584b6d7f6b06e1ed871`, exit `0` (clean).
- `git fetch --no-tags origin +refs/heads/main:refs/remotes/origin/main`: cwd `/home/do/git/d-o-hub/do-harness`, updated `origin/main` to `fbd3f4fef682c9cbeebefb5a018c855e0ef6aadf`, exit `0`.
- `git merge --ff-only origin/main`: cwd `/home/do/git/d-o-hub/do-harness`, fast-forwarded `main` to `fbd3f4fef682c9cbeebefb5a018c855e0ef6aadf`, exit `0`.
- `git checkout -b chore/latest-main-guardrails`: cwd `/home/do/git/d-o-hub/do-harness`, created branch at `fbd3f4fef682c9cbeebefb5a018c855e0ef6aadf`, exit `0`.
- `bash .agents/skills/harness/scripts/latest-main.sh --read-only`: cwd `/home/do/git/d-o-hub/do-harness`, exit `0`, emitted `fbd3f4fef682c9cbeebefb5a018c855e0ef6aadf`.
- `bash .agents/skills/harness/scripts/latest-main.sh`: cwd `/home/do/git/d-o-hub/do-harness`, exit `0`, emitted `fbd3f4fef682c9cbeebefb5a018c855e0ef6aadf`.
- `bash .agents/skills/issue-to-pr/scripts/preflight.sh --dry-run`: cwd `/home/do/git/d-o-hub/do-harness`, exit `0`.
- `bash .agents/skills/pr-triage/scripts/preflight.sh --dry-run`: cwd `/home/do/git/d-o-hub/do-harness`, exit `0`.
- `python3 .agents/skills/skill-creator/scripts/quick_validate.py .agents/skills/harness`: exit `0`, valid.
- `python3 .agents/skills/skill-creator/scripts/quick_validate.py .agents/skills/htn-planner`: exit `0`, valid.
- `python3 .agents/skills/skill-creator/scripts/quick_validate.py .agents/skills/issue-to-pr`: exit `0`, valid.
- `python3 .agents/skills/skill-creator/scripts/quick_validate.py .agents/skills/pr-triage`: exit `0`, valid.
- `python3 scripts/tests/test_latest_main.py -v`: exit `0`, 7 comprehensive tests passed:
  1. `test_remote_advancement`: bare origin advanced A->B, helper fetches B into `refs/remotes/origin/main`, local `HEAD` remains A.
  2. `test_read_only`: queries commit B via `ls-remote`; object B remains absent locally; `FETCH_HEAD`, `refs`, `index`, `HEAD`, and `status --porcelain` snapshotted before and proven byte-for-byte identical after.
  3. `test_dirty_feature_branch`: staged, unstaged, and untracked file sentinels preserved byte-for-byte; branch `feat-1` intact; remote metadata advances.
  4. `test_correct_target_with_git_env`: `--root` target Y advances while `GIT_DIR` and `GIT_COMMON_DIR` point to foreign checkout X.
  5. `test_failure_closed`: empty directory, missing origin remote, missing remote main on bare origin, unreachable origin URL, and invalid/missing arguments all exit 2 with no stdout SHA; git shim confirms malformed 41-char hex ID, extra columns (3+ tokens), and multiple lines fail closed with exit 2.
  6. `test_start_item_dry_run_and_advance`: permanent hermetic test verifies that new dry-run emits advertised SHA B with `(advertised only; not fetched)` without branch mutation, new branch cuts at commit B, resume dry-run observes commit D, and normal resume refreshes `origin/main` to commit D while preserving existing branch work at commit C without rebase.
     - *One-off baseline reproduction (historical `fbd3f4f` `start-item.sh`):* Executed in scratch Git fixture. Old dry-run output: `dry-run: issue #62 -> branch issue-62-document-retry-policy from origin/main@194b14a6445ccc82e3dc769bf417a476b50c0ac3` (stale commit A, missing commit B `596816d`). Old resume output: `resumed: issue #62 on branch issue-62-document-retry-policy`, `origin/main` remained at `596816d` (missing remote commit D `79661e0`). This empirically proved the stale dry-run and unrefreshed resume gaps before cutover.
  7. `test_preflight_scripts`: dry-run and normal runs proven for BOTH `issue-to-pr` and `pr-triage` preflights; preflight in dirty state preserves dirty sentinels byte-for-byte on both scripts; missing/broken origin fails closed with exit 2; missing `latest-main.sh` helper fails closed with exit 2.
- `CARGO_BUILD_JOBS=2 CARGO_INCREMENTAL=0 cargo test -p do-harness eval::fixture_tests`: exit `0`, 3 passed (dataset quality, strict thin fixture rejection, rich fixture acceptance).
- `cargo test -p do-harness-types && cargo test -p do-harness-db`: exit `0`, 125 passed across 7 test suites.
- `cargo build --release -p do-harness`: exit `0`, built release binary `do-harness 0.4.1 (fbd3f4f-dirty 2026-10-31)`.
- `do-harness eval --strict-fixtures --skill guide-only-control --agent-cmd true --format json` (throwaway negative control workspace): exit `1`, confirmed refusal `FIXTURE-MISS: no assertion reads executor output (agent_stdout.txt) or artifact the agent must create`.
- Adopting workspace smoke test (`init --language generic` in temporary bare-origin repo): exit `0`, scaffolded `.agents/skills/harness/scripts/latest-main.sh`, executed normal fetch and `--read-only` modes cleanly with preserved worktree and HEAD.
- `do-harness eval --strict-fixtures --skill htn-planner`: exit `0`, passed (lift +0.44).
- `do-harness eval --strict-fixtures --skill pr-triage`: exit `0`, passed (lift +0.22).
- `do-harness eval --strict-fixtures --skill harness`: exit `1`, `LIFT-MISS: lift +0.06 below blessed floor +0.28`. Blocked by `raise_lift_floor`'s raise-only database contract (`WHERE excluded.floor > skill_lift_floors.floor`), where removing obsolete copied-guidance assertions dropped deterministic lift below grandfathered local state.
- `do-harness eval --strict-fixtures --skill issue-to-pr`: exit `1`, `LIFT-MISS: lift +0.00 below blessed floor +0.01`. Same deterministic lift floor barrier.
- `do-harness verify --only latest-main-guardrails --only skills --only check-invariants`: exit `0`, all 3 sensors passed.
- `do-harness verify --only fmt --only check --only clippy --only loc --only docs-sync`: exit `0`, all 5 sensors passed.
- `do-harness verify --only docs-examples --only docs-links --only markdownlint`: exit `0`, all 3 sensors passed.
- `do-harness verify --only shell --only yamllint --only actionlint --only action-pins --only check-agt --only check-mcp --only untrusted-ingest --only privacy`: exit `0`, all 8 sensors passed.
- `do-harness verify --only repo-links --only release-notes --only release-manager --only ci-concurrency-guardrails --only ci-workflow-costs --only latest-main-guardrails`: exit `0`, all 6 sensors passed.
- `do-harness verify --only deps --only machete --only audit --only doctest`: exit `0`, all 4 sensors passed.
- `CARGO_BUILD_JOBS=1 CARGO_INCREMENTAL=0 NEXTEST_TEST_THREADS=1 do-harness verify`: exit `0`, all 40 configured sensors passed in 1795.97s (fmt, check, clippy, test [1273 tests], doctest, coverage [findings 0], loc [findings 0], skills, skills-docs, deps, shell, markdownlint, yamllint, actionlint, action-pins, docs-sync, docs-examples, docs-links, check-agt, check-mcp, check-invariants, check-promotions, audit, magic-numbers [delta +0], commitlint, privacy, untrusted-ingest, repo-links, release-notes, release-manager, release-manager-e2e, publish-npm, verify-published, dora [delta -3], powerset, machete, ci-concurrency-guardrails, workflow-benchmark, ci-workflow-costs, latest-main-guardrails).

## References and follow-ups

- Follow-up records: [plans/analysis-followups.md#post-v041-codebase-review-2026-10-31](analysis-followups.md#post-v041-codebase-review-2026-10-31)
  - `FU-01`: Migration-backup consistency under concurrency (`crates/db/src/migrate.rs:299-319`).
  - `FU-02`: Web-UI locale audit direction check order (`integrations/web-ui/lib/i18n-audit.mjs:99-117`).
  - `FU-03`: Web-UI advertised route × viewport × locale × scheme matrix (`integrations/web-ui/README.md:4`, `integrations/web-ui/lib/audit.mjs:102-125`).
- Published release baseline: [do-harness v0.4.1 Release](https://github.com/d-o-hub/do-harness/releases/tag/v0.4.1)
- Upstream Git official documentation:
  - [git-fetch 2.53.0](https://git-scm.com/docs/git-fetch/2.53.0)
  - [git-pull 2.53.0](https://git-scm.com/docs/git-pull/2.53.0)
  - [git-ls-remote 2.48.0](https://git-scm.com/docs/git-ls-remote/2.48.0)
  - [git-status 2.53.0](https://git-scm.com/docs/git-status/2.53.0)
- SQLite official documentation:
  - [SQLite PRAGMA wal_checkpoint](https://sqlite.org/pragma.html#pragma_wal_checkpoint)
  - [SQLite Backup API](https://sqlite.org/backup.html)
- Playwright official documentation:
  - [Playwright Emulation: Color scheme](https://playwright.dev/docs/emulation#color-scheme-and-media)
