# Codebase analysis follow-ups (2026-10-15 swarm)

> **Status:** analysis complete; findings verified against source, fixes not yet implemented
> **Related:** release distribution, persistence/db invariants, guardian-proxy threat model, sensors/CI policy, test coverage
> **Created:** 2026-10-15

## Method

A six-agent read-only swarm reviewed non-overlapping domains, and each HIGH
finding was independently spot-checked against source (or the crates.io API)
before entering this record, per the `harness` skill's swarm handoff protocol.
Version-sensitive dependencies were checked against official documents
(crates.io registry API, `modelcontextprotocol/rust-sdk`, the axum 0.8
announcement, cargo-deny/cargo-audit workflows). No computational sensor ran in
the analysis environment (no Rust toolchain, no `gh`); the implementation phase
must re-run `do-harness verify --set verification --strict` in a dev checkout.

Findings are grouped by area and carry `file:line` evidence and an effort
estimate (S/M/L). IDs: `R` release-state, `A` correctness, `B` LOC ceiling,
`C` duplication, `D`/`T` tests, `E` performance, `S` schema/type contracts,
`F` supply chain, `P` policy/CI.

## Implementation progress (2026-10-15)

A first implementation pass landed the items below, each verified with the real
toolchain (Rust 1.99.0, clippy, shellcheck, yamllint, markdownlint-cli2) in a
dev checkout.

| Item | Change | Verification |
|------|--------|--------------|
| A1 | `doctor` runs the event-chain audit in **both** output modes; JSON gained `event_chain` and now reports `"ok": false` on a tampered chain | `cargo test -p do-harness --bin do-harness doctor` → 8 passed (incl. new `fails_on_tampered_event_chain_json`); workspace `cargo clippy --all-targets -- -D warnings` clean |
| A2 | `backup_state_file` checkpoints the WAL (`PRAGMA wal_checkpoint(TRUNCATE)`) before copying, so the pre-migration snapshot is complete | `cargo test -p do-harness-db` → 66 passed (incl. rewritten `backup_state_file_is_wal_consistent`) |
| A3 | `advance_subtask` and `update_task_status` exclude terminal tasks in their `WHERE ... RETURNING`, so advancing/completing a `done`/`failed` task fails closed (`InvalidTaskTransition`) instead of appending an out-of-order event; new `DbError::InvalidTaskTransition` + 2 tests | `cargo test -p do-harness-db` → 66 passed; workflow integration tests (`beat_scope`/`signals`/`status`/`concurrency`/`audit_recovery`) → 23 passed |
| A5 | `bless_grader_baseline` now uses `begin_immediate`; no production `.transaction()` (deferred) call remains in the crate, so the `tx.rs` invariant holds | `grep` clean; `cargo test -p do-harness-db` → 66 passed |
| A7 | `upstream_client()` sets `reqwest::redirect::Policy::none()` — an allowed upstream can no longer answer `302` to a metadata address | `cargo test -p guardian-proxy` → 40 passed; clippy clean |
| A8 | `normalize_host_literal` maps IPv4-mapped IPv6 (`::ffff:169.254.169.254`) to its embedded IPv4, `::` to `0.0.0.0`, and WHATWG-style IPv4 literals (`0x7f000001`, `2130706433`, `017700000001`) to a dotted quad before the range checks; 5 new tests | `cargo test -p guardian-proxy --lib` → 42 passed |
| S4 | `struck_signatures` does the fail-fast pre-check in one indexed query instead of one `get_error_signature` per sensor; `struck_sensors` rewired to it | `cargo test -p do-harness-db` → 66 passed; `cargo test --bin do-harness -- telemetry errors` → 20 passed |
| A4 | `record_eval_run` writes the latest-per-skill row, the history row and the dimension rows in one transaction; the orchestrator uses it instead of three independent writes | `cargo test -p do-harness-db` → 67 passed |
| C4 | `classify_verdict` + `cancelled_result` are shared by the parallel and sequential execution paths, so the verdict ladder cannot drift | `cargo test --bin do-harness -- sensors` → 34 passed |
| R4 | AGENTS.md synced: fifth DORA metric, npm platform-package clarification, accurate CI gate list + named jobs, `check-mcp` added | markdownlint clean |
| R5/R6 | `docs/compliance.md` names the *assessed* version; README hook command shows `--record`, the CI paragraph matches `verify.yml`, and the `docs/cli.md` skills-suggest example shows the real catalog size | markdownlint clean |
| R9 | `check-skills-sync.sh` fails when the two `anti_ai_slop.md` copies diverge | guard fires in a hermetic scratch tree |
| C1 | one `util::percentile` (with a covering test) shared by the DORA collector and the PR-loop metrics | `cargo test --bins` → 553 passed |
| S5/T18 | in-crate tests for `repo_sensors` (the `MAX` ratchet monotonicity and the `previous_max` bless flow) | `cargo test -p do-harness-db` → 69 passed |
| S8 | `parse_invariants_json` returns a typed `ParseInvariantsError` (its `Display` keeps the expected-schema guidance verbatim) | `cargo test -p do-harness-types` → 19 passed |
| S19 | the `tx.rs` ordering test now describes what it actually asserts | `cargo test -p do-harness-db` → 69 passed |
| S9/S11 | `DbError::Constraint` carries the extended `SQLite` code; `is_unique_violation()` matches UNIQUE/PRIMARY KEY/ROWID only, and the seq-retry loop retries just that race instead of every constraint | `cargo test -p do-harness-db` → 71 passed |
| S10 | impossible-branch `NotFound` sites (10 of them) became `DbError::Invariant`; a failing task lookup in `missing_row_error` is propagated instead of surfacing as "task not found" | `cargo test -p do-harness-db` → 71 passed |
| S18 | an unknown stored eval mode is `DbError::InvalidEvalMode`, not a silent degrade to `deterministic` (which would misreport an agent run in the lift ratchet) | `cargo test -p do-harness-db` → 71 passed |
| C5 | one rule for pre-check failures (`exit::UsageError` + `exit::classify`): a failure before any check runs exits 2, a check that ran and failed exits 1, and an unmarked error defaults to verify. `doctor` no longer disagrees with `hook status` about a missing git repo, and `eval` no longer disagrees with `task` about an unopenable database | `cargo test -p do-harness --test exit_classification` → 3 passed; `--bins` → 556 passed; can-fail proof: reverting the two `map_err(exit::classify)` sites gives 0 passed / 3 failed |
| S13 | the four paging APIs take `Option<u32>` limits (and `u32` offsets) instead of the `limit = -1` sentinel; `bind_limit` maps `None` to the negative `LIMIT` SQLite documents, and a new test pins `None` (every row), a window, and `Some(0)` (no rows) | `cargo test -p do-harness-db` → 73 passed |
| S14 | `latest_sensor_beat` returns `LatestSensorBeat` with named fields instead of a 4-tuple whose `status` and `input_digest` are both strings | `cargo test -p do-harness-db` → 73 passed; `cargo test -p do-harness --test unchanged --test unchanged_edges` → 16 passed |
| S12 | the unchanged-input digest is bound in the `INSERT` (`insert_beat_with_digest`) instead of a post-insert `UPDATE`, so a beat row never exists without the digest of the inputs it was recorded against | `cargo test -p do-harness-db` → 73 passed |
| S16 | migration `0023` indexes `heuristics(skill_name)`; the query-plan test shares the shipped statement (`LIST_BY_SKILL_SQL`) so the plan under test is the plan that ships, proving the read seeks the index and that `ORDER BY id` needs no second column (rowid trailer) | `cargo test -p do-harness-db` → 74 passed; can-fail proof: dropping the index yields `SCAN heuristics` → 0 passed / 1 failed, and ordering by an unindexed column yields `USE TEMP B-TREE FOR ORDER BY` → 0 passed / 1 failed |
| S15 | serde contracts pinned for the types that cross a boundary — the hand-edited `plans/methods.json` (nested `Subtask` unknown-field rejection; `spike_candidate` and `preconditions` required), the `EvalDim`/`EvalMode` wire names (`serde` name == `as_str()` == `FromStr`), and the file-level `MethodFile` contract (unknown top-level key errors, missing file falls back to the embedded catalog) | `cargo test -p do-harness-types` → 24 passed; `cargo test -p do-harness --bins` → 558 passed |
| T4 | the v3/v4 evidence-compatibility test pins the observed outcome instead of accepting "exit 0 or 1": a readable v3/v4 file is `stale` with reason `workspace_changed` (the reader parsed it and only the fingerprint differs), and a v2 file is `missing` with reason `legacy_schema` — each with its exact exit code | `cargo test -p do-harness --test release_contract` → 5 passed; the pinned values come from a faithful repro of the fixture workspace (`--root` + the minimal policy), not from guessing |
| T7 | `--no-tests=pass` removed from the repository policy *and* from the built-in pack `init` uses, so a suite that shrank to zero fails the `test` sensor instead of reporting green. Both consequences are pinned: the pack argv holds on every host (`config::builtin_rust_pack_test_sensor_fails_on_zero_tests`), and adoption is now honest — an existing crate adopted without tests ends with a red baseline naming `test` (`init_bootstrap::existing_rust_repo_without_tests_reports_the_empty_test_gate`) while a crate that has one still baselines green (`init_bootstrap::existing_rust_repo_is_not_rewritten`, whose fixture gained a unit test). `tests/vacuous_gates.rs` proves the behaviour end to end where cargo-nextest exists (green for the scaffolded suite, red once every test is deleted, `test` named in `failed`); `docs/cli.md` and `docs/adoption.md` state the adoption contract | `cargo test -p do-harness --bins config::tests` → 22 passed; `cargo test -p do-harness --test init_bootstrap` → 5 passed; `cargo test -p do-harness --test vacuous_gates` → 1 passed |
| T9 | partly stale: the `db:` passing path is already covered at the grader level (`eval_assert::db_assertion_valid_identifiers_grade_against_real_db` grades a seeded temp database), so the runner-level gap was the *opposite* contract — grading is hermetic (`Sandbox::for_skill` mirrors only the skill, never `.do-harness/`), which the new test pins by seeding the caller's database and asserting the assertion still grades 0.0 | `cargo test -p do-harness --bins` → 559 passed |
| T8 | the four cargo-tool parity checks (`audit`/`deps`/`powerset`/`machete`) no longer have a skip path: each script runs with a private `bin` as its whole `PATH`, so a host-installed tool cannot change the outcome, and every check asserts the same policy (`SKIP:` + exit 0 locally, `FAIL:` + exit 1 under `CI=true` or `DO_HARNESS_REQUIRE_TOOLS=1`). `check-deps.sh` needs `grep` and `bash` in that bin because it greps a manifest and shells out to `check-package-contract.sh` before reaching its missing-tool branch; the four bash-driven checks are `#[cfg(unix)]` like the lint ones | `cargo test -p do-harness --test ci_parity` → 6 passed; can-fail proof: making the local `check-audit.sh` branch exit 3 fails the check (`local run must SKIP`) and restoring it passes |
| T10 | `.codecov.yml` removed: no workflow uploads coverage to Codecov and no Codecov check appears on a PR, so its 70% project / 60% patch targets gated nothing while the repository's own `scripts/check-coverage.sh` (70%, `coverage` sensor) is the floor that runs. Codecov-comment handling in `pr/waiver_comment.rs` reads PR comments and is unaffected | `gh pr checks 320` (no codecov check); no workflow or doc names the file |
| T17 | resolved the opposite way to the finding's implication — the dead `[profile.ci]` is *removed* rather than wired, because wiring it was attempted and rejected on evidence: with `NEXTEST_PROFILE=ci` set, every nested nextest run must resolve a profile, and `init` writes `.config/nextest.toml` **only when it scaffolds a crate**, so an adopted workspace (whose pack still carries a `test` sensor) dies with "error: profile `ci` not found". Reproduced locally against the adoption fixture: plain env green, `NEXTEST_PROFILE=ci` red with `failed: ["test"]`. `.config/nextest.toml` now records the decision: no named profile, CI keeps `retries = 0`, and the intermittent Windows crash a retry would have absorbed stays mitigated by serializing the db crate | `cargo nextest list -p do-harness --test ci_parity` (config parses); `cargo test -p do-harness --test embedded_assets` → 2 passed |
| T16 | the public wire contracts carry doctests again: `EvalDim` (the one vocabulary fixtures, the database, and reports share), `EvalMode` (`deterministic`/`agent` plus the default), `DecisionHeader` (serde shape and `deny_unknown_fields`), and `parse_invariants_json` (both accepted top-level shapes and the rejected one). The workspace had exactly one doctest (`chain_hash`), and AGENTS.md section 4 names a doc-test on a public API as how a real check stays alive; the examples are also the documentation of those wire names | `cargo test -p do-harness-types --doc` → 5 passed (was 1); `cargo test -p do-harness-types` → 23 passed |
| T15 | the two benchmark corpora can no longer drift apart: a test reads both manifests and both case directories, requiring the same case names with the same `expected_min_route` floors and the same directories on disk. `pr_routing.rs`'s own doc calls `pr-routing-large` "the same 12 classes at realistic size", and nothing read both side by side, so a case added to one corpus alone kept every assertion green while the harder variant stopped measuring the same problem | `cargo test -p do-harness --test pr_routing` → 8 passed |
| T12 | nested-build-isolation Rule 3 is a check instead of a post-coverage ritual: a suite test walks the source tree (pruning build output, harness state, and dependency caches; skipping symlinks so a cyclic link cannot loop) and fails on any `*.profraw` beside sources, naming each file. The skill's manual step was `find . -name '*.profraw' \| wc -l` after an instrumented run, and those files are gitignored, so nothing else in the repository noticed a leak | `cargo test -p do-harness --test profraw_litter` → 1 passed; can-fail proof: planting `crates/do-harness/leak.profraw` fails the test with that path, removing it passes |
| T5 | `collect_skill_trends` — the joins behind every number `metrics` prints — now has assertions: two seeded runs pin the latest rate (from `skill_evals`), the best-ever rate and run count (from the run history), the blessed bar and lift floors, Skill Lift computed on the newest run only, the cost proxies, and per-dimension rates with and without the skill (ordered by dimension, same denominator). Two further cases pin the `None` semantics a report must not show as `0.0`: no measured baseline means no lift, and a run or dimension that graded nothing has no rate at all | `cargo test -p do-harness --bins skill_trends` → 3 passed; can-fail proof: flipping `with - without` to `without - with` fails the lift assertion and restoring it passes |
| T18 | `repo_metrics.rs` gets direct tests (`repo_sensors.rs` already had them, so only this half was open — the record listed T18 as both landed and not started, and this closes that inconsistency): `sensor_stats` for grouping and ordering, non-`ok` counting, excluding beats that are not named sensor beats, `since`, `scope`, and an unused scope reporting nothing rather than everything; `has_ok_beat` for the empty table, failures-only, and a success of any beat type. Writing them surfaced the schema's CHECK `beat_type != 'sensor' OR sensor_name IS NOT NULL`, now pinned as well (a nameless sensor beat is rejected outright) | `cargo test -p do-harness-db` → 77 passed (was 74) |
| T14 | the managed `pre-push` hook is executed, not just read: an e2e test installs the hooks, commits, pushes to a bare local remote (a green workspace pushes), then breaks the crate and pushes again (the `check` sensor fails, the push is refused, and the hook output names `check`). `hooks/tests.rs` asserts the installed script by string match, and this is the only place it runs under a real `git push`; the refused push is the can-fail proof, since a hook that stopped gating would let it through | `cargo test -p do-harness --test hook_e2e` → 3 passed |
| P11 | the `feedback` set now names every sensor the generated `pre-commit` hook runs (`loc` was in the hook and missing from the set, in both the repository policy and the generated default), and the `commitlint` sensor's lack of `when-changed` is documented in place — a commit message is not tied to a tracked path, so it is always applicable. A guard loads both configs and asserts the pairing | `cargo test -p do-harness --bins init::tests` → 42 passed (was 41); can-fail proof: dropping `loc` from the repository policy fails the guard with "the fast loop must include loc, which the pre-commit hook runs" |
| T3 | `dora --source gh` is driven end to end against a fake `gh` on `PATH`: the release-run fold (a success anywhere in a head's runs wins regardless of run order), windowed counters against **unwindowed** reconciliation (a success before `window_start` still counts as shipped, while a head that only ever failed is named in `deploy_head_mismatches`), nearest-rank PR lead-time percentiles, `null` rather than `0` for no samples, and a failing `gh` surfacing as an error instead of a green empty snapshot. The inline tests covered timestamp parsing only, and this is the path that talks to the network in production. The file is `cfg(unix)` because the fake `gh` is a bash script | `cargo test -p do-harness --test dora_gh` → 3 passed |
| P1 | the `verify` job no longer runs the guardian-proxy crate twice: the raw `cargo check -p guardian-proxy` and the three-invocation `cargo test` step are gone, because the sensors already cover them — `check` compiles the crate with default features, the `test` sensor's workspace run runs its default-feature suite, and `check-agt`/`check-mcp` build both feature matrices. The duplicate step was also the one that always ran, since only the sensors carry `when-changed` | `bash scripts/check-yamllint.sh` → 13 files; `explain --set verification` still selects both feature sensors |
| P4 | the `scheduled` set moved to its own nightly workflow (`.github/workflows/scheduled.yml`, `cron: "23 4 * * *"` plus `workflow_dispatch`): `powerset`'s four feature builds were charged to every push and pull request from inside `verify.yml`, and the set's evidence was neither asserted nor uploaded. The new job runs `verify --set scheduled --strict` with `CI=true`/`DO_HARNESS_REQUIRE_TOOLS=1` and then asserts `status --set scheduled`. `machete` stays at pull-request time — it is a single scan and a diff can add an unused dependency — so `scheduled` is now `["powerset"]` and `machete` joined `verification` | `explain --set scheduled` selects powerset only; `explain --set verification` selects 22 sensors, machete among them |
| P9 | the `source-distribution` job got `Swatinem/rust-cache` (it recompiled the workspace for its target on every run), and the verify job's Playwright Chromium download is cached (`actions/cache@v4` over `~/.cache/ms-playwright`, keyed by runner OS and the pinned 1.50.1) instead of re-fetching about 150 MB per run | `bash scripts/check-yamllint.sh` → 13 files, 0 issues; both caches reuse the SHAs this workflow already pins |
| P14 | `.pre-commit-config.yaml` resolves the binary instead of hardcoding `./target/release/do-harness`: `DO_HARNESS_BIN` first, then `${CARGO_TARGET_DIR:-target}/release/do-harness`, so a redirected target directory cannot make the hook execute a stale or missing binary. `README.md` documents the resolution and what the integration *is* — a second entry point for pre-commit users, not a second gate (the check is already in the `feedback` set, and the managed hook's execution is covered end to end by `hook_e2e`) | `bash scripts/check-yamllint.sh` → 13 files; `markdownlint-cli2 README.md` → 0 issues |
| S17 | resolved by evidence rather than by a newtype: every derivation guards its division (`graded > 0` in `dim_rate` and the run rates, `.max(1)` in the PR-loop shares), so a `NaN` cannot enter through the arithmetic and the ratchet's ordering comparisons cannot see one. A test now pins that from the outside at both extremes and at the empty denominator, and the finding row records the correction; a `PassRate`-style newtype stays available if a rate ever arrives from outside the derivation | `cargo test -p do-harness --bins skill_trends` → 4 passed; can-fail proof: removing the `graded > 0` guard fails the test with `nothing-graded: correctness rate` and restoring it passes |
| P13 | workflow hygiene: `verify.yml` gained `workflow_dispatch` (a transient runner failure no longer needs an empty commit to produce fresh evidence) and dropped `cargo-llvm-cov` from the verify job: CI ran that job without the tool and every sensor passed, `test` included — which is the dogfood path — so the tool is genuinely unused there. The windows job's list keeps it because no equivalent run exists. Two corrections belong to this batch: its own CI failure was clippy's pedantic `cast_precision_loss` on the new S17 test (`i64 as f64`), which now converts through `u32` like `pr_loop.rs` does; and an earlier draft of this row blamed the tool removal for that failure, which the same log disproves; `contributor-check.yml` gained a per-issue/pull-request concurrency group and stopped interpolating the third-party action's output, which derives from contributor-controlled data, into `run:` (it reaches the shell through the environment now); `selection-impact-benchmark.yml` resolves its toolchain from `rust-toolchain.toml` instead of hardcoding `1.99.0`, drops persisted credentials, and gains concurrency. The one remaining P13 nit — pinning `install-action` tool versions — is a maintenance-cadence decision (only `cargo-semver-checks@0.50.0` is pinned, because its output feeds policy) and stays open | `bash scripts/check-yamllint.sh` → 13 YAML files, 0 issues; the workflow triggers and concurrency expressions were parsed back out with a YAML parser |
| T11 | `tests/selection_impact_test.sh` no longer trusts its binary or the committed fixture: it fails closed with a remedy when `DO_HARNESS_BIN`/`target/release/do-harness` is missing **or older than the sources**, since the suite's assertions describe that binary's behaviour; and the two planted-defect tests mutate a throwaway copy of the fixture workspace (`mktemp -d` + `cp -R`, removed by an `EXIT`/`INT`/`TERM` trap) instead of a committed tree a SIGKILL or a cancelled CI step would leave corrupted | `bash scripts/check-shell.sh` → 68 scripts; full suite prints `ALL SELECTION IMPACT TESTS PASSED SUCCESSFULLY!`; missing binary → exit 1, stale binary → exit 1; proof of the moved target: with both restore writes replaced by `pass` the run still passes and `git status tests/fixtures/selection-impact` stays empty |
| T1 | `coverage` moved into the `scheduled` set, which runs nightly from `.github/workflows/scheduled.yml` — a job that already asserts its evidence with `status --set scheduled`. The floor sat in `release` only, so a regression could land on main and surface at tag time. Nightly rather than per-PR is deliberate: the instrumented run is the heaviest sensor in the repository and AGENTS.md records the OOM an uncapped full verify caused; the job gained `cargo-llvm-cov`/`cargo-nextest` (both fail closed there) and a 45-minute timeout | `explain --set scheduled` selects `powerset` and `coverage`; `bash scripts/check-yamllint.sh` → 13 files |
| T2 | measured rather than assumed, and the finding does not reproduce for this report: `cargo llvm-cov nextest --workspace` (caps `CARGO_BUILD_JOBS=2`, `test-threads = 2`) reports 142 source files and **none** under `tests/`, `fixtures/`, or `*_tests.rs`, so the harness's test *files* are already outside the number; unfiltered line coverage is 84.76% against the 70% floor. A `--ignore-filename-regex` for those paths was added, measured to move the total by about 1% (nothing a filter can address), and removed again rather than shipped as dead config. The residual the finding is really pointing at is the 91 `src` files carrying an **inline** `#[cfg(test)]` module, which no path filter can exclude; marking those (`LCOV_EXCL_START`/`STOP`) is a maintenance policy rather than a defect, and is recorded as such. The measured run also left no `.profraw` litter, which the T12 gate checks | `cargo llvm-cov report --summary-only` → 142 rows, 0 test-path rows, TOTAL 84.76% lines |
| R3 | `man/do-harness.1` is generated, guarded, and no longer stale: the CLI's `man` command renders through a new `man` module (which also rewrites `clap_mangen`'s `do-harness-<name>(1)` index — the repository ships one page, so those 30 refs pointed at pages that are never generated, and now read `do-harness <name>`), and `tests/man_sync.rs` regenerates the page and compares it against the committed copy with only the build fingerprint (`<rev>[-dirty] <date>`) normalised away. The semver stays compared, which is the drift nobody noticed (the page announced `0.2.0 (01f6e3e-dirty 2026-10-03)` while the crate was `0.3.0`). `docs/releasing.md`'s runbook now says to regenerate both generated artifacts before tagging, and extracting the renderer took `main.rs` from 468 to 462 lines | `cargo test -p do-harness --test man_sync` → 1 passed; can-fail proof: restoring the stale `0.2.0` fingerprint fails it with "man/do-harness.1 is stale; regenerate with `do-harness man man`" |
| R2 | the interim annotation the finding prescribes, since the real fix is the stranded release (R1, operator-gated): `docs/releases/v0.3.0.md` now opens with the publication status — the tag exists on `6804b41`, crates.io's newest `do-harness` is still 0.2.1, and the release job stopped before its prebuilt artifacts — points at R1 for the recovery, and says to install the last published version or build from a checkout meanwhile. The README's source-install paragraph already carried the equivalent caveat ("use the tested prebuilt artifacts above until 0.3.0 is published") | `markdownlint-cli2 docs/releases/v0.3.0.md docs/releasing.md` → 0 issues |
| R7 | the epic→heuristic citations resolve now. `plans/mcp-conformance-epic.md` cited "harness heuristic 18/19/20/21/22", a counter that lived only in the author's session: the harness file's bullet order never matched those numbers and `fail-closed-proxy`'s file holds four entries, so nothing in the tree could reconstruct them. Each citation names the file plus the lesson it stands for, matching the entries' own wording, and the two files exist. The `skills-docs` sensor gained the matching guard: a `plans/*-epic.md` may not cite a numbered heuristic, and every cited `…/references/heuristics.md` must exist on disk. The trace-ID half of the finding is a local-database convention (ids skip because strikes consumed numbers) and stays a process note | `bash scripts/check-skills-docs.sh` → OK, 13 skills; can-fail proofs: a numbered citation fails naming file and line, a dangling citation fails naming the path |
| P16 | the high-value half of the script-polish bundle: `check-yamllint.sh` and `check-markdownlint.sh` now collect **tracked and untracked** files (`git ls-files --cached --others --exclude-standard`; ignored trees stay excluded). A new workflow or document used to be unlinted until it was staged — exactly when a syntax error is cheapest to catch — and it bit this batch: the new `scheduled.yml` was reported as "12 files linted" while untracked. `check-loc.sh` no longer hides `find` errors (the `2>/dev/null` is gone), so an unreadable tree surfaces instead of silently shrinking the scan. The rest of the bundle (unguarded `$2` under `set -u`, FAIL/SKIP lines without hints, an inconsistent python3-availability policy) stays open | `bash scripts/check-shell.sh` → 68 scripts; both sensors pass clean (13 YAML, 89 markdown) and fail on an untracked bad file: a YAML syntax error → exit 1, `#Title` → `MD018/no-missing-space-atx` → exit 1 |




| P2/P3 | Toolchain/lint/nextest configs and each sensor's own script added to the `when-changed` inputs whose verdicts they change | `do-harness list`/`explain --set verification --changed` run against the rebuilt binary; TOML parses (25 sensors) |
| R8 | `pr-triage` SKILL.md step 10 and description mark semantic routing optional/experimental with the measured no-go | markdownlint clean; `untrusted-ingest`/`skills-docs` sensors pass |
| F5 | `.clippy.toml` gained `msrv = "1.85"` (and `IPv4`/`IPv6` as valid doc idents) | workspace clippy clean; the setting surfaced a real `doc_markdown` finding during the pass |
| P5 | `check-shell.sh` recurses `scripts/**` and lints `tests/**/*.sh` (dropped `-maxdepth 1`) — two previously-unlinted CI-executed scripts are now covered | `bash scripts/check-shell.sh` → 68 scripts, exit 0 (both new files pre-checked clean) |
| P6 (partial) | Fail-closed-on-zero added to `check-markdownlint.sh` + `check-yamllint.sh`; `check-untrusted-ingest.sh` no longer skips before its contract check when the skill is present but `scripts/` is gone | hermetic scratch tree: zero-file → FAIL/1; skill-present-scripts-absent → FAIL/1 |
| B2 (forced) | A1 pushed `doctor.rs` to 562 lines; the inline test module was extracted to `doctor_tests.rs` via the `#[path]` split | `scripts/check-loc.sh` → `FINDINGS: 0`; doctor tests still 8 passed |

**Correction:** the P6 claim that `check-skills-docs.sh` "passes vacuously on an
empty catalog" was **wrong** — `set -o pipefail` already failed it (exit 2) via
the glob-grep. The change makes the failure a clear message (via `nullglob` +
explicit guard) instead of a raw grep error.

**Residual:** none for A8 (alternate IPv4 spellings are now normalized).

**Not started:** R1 (release rescue), A10 (pre-push set), A6, S1–S3, S6, S7,
T6, T13, F1–F4, P7–P8, P10, P12, P15, P16 (partial), B1, B3–B5, C2, C3,
D1–D2.

## Progress: release-state truth decay (verified first-party)

| ID | Severity | Finding | Evidence | Action | Effort |
|----|----------|---------|----------|--------|--------|
| R1 | **critical** | The `v0.3.0` release is stranded mid-flight. The tag exists on `6804b41`, but its publish job failed (`do-harness-libsql@0.9.30 already exists`, retried 8× — the `published()` curl check hit a malformed URL, fixed in #312/`787c19f` *after* the tag), so `registry-validation`/`npm-publish`/`release` skipped. `do-harness-libsql@0.9.30` was published **manually** by user `d-o-hub` (crates.io API: `trustpub_data: null`); future tag runs will fail the OIDC exchange until Trusted Publishing is configured | `git log -1 v0.3.0` → `6804b41`; crates.io `/api/v1/crates/do-harness-libsql`; `docs/releasing.md` recovery runbook | Configure Trusted Publishing for `do-harness-libsql`, re-cut the tag on `787c19f`, verify the release completes, then sync roadmap text | M |
| R2 | high | The documented install command fails today: `README.md` and `docs/releases/v0.3.0.md` instruct `cargo install do-harness --version 0.3.0 --locked`, but crates.io max is 0.2.1; the curated v0.3.0 notes read as *shipped* | `README.md`; crates.io `do-harness` | Finish R1; until then annotate `docs/releases/v0.3.0.md` | S |
| R3 | med | `man/do-harness.1` is stale (embeds `0.2.0 (01f6e3e-dirty 2026-10-03)`) and has **no drift guard** (unlike `completions/`, guarded by `completions_sync.rs`); its SUBCOMMANDS cross-references `do-harness-verify(1)` etc. are never generated | `man/do-harness.1:3`; `main.rs` single-page `man` | Add a `man_sync` test mirroring `completions_sync.rs`; regenerate; emit subcommand pages or strip the `(1)` refs | S–M |
| R4 | med | `AGENTS.md` accuracy gaps: lists four DORA metrics (omits deployment rework rate, #309); npm "four platform packages" adjacent to the refused `win32-x64`; CI summary misstates the guardian-proxy check scope and omits the `scheduled` set, `dora --record`, and four jobs; sensor enumeration omits `check-mcp` | `AGENTS.md` §1/§4 | One sync pass (feedforward contract read by every session) | S |
| R5 | low | `docs/compliance.md` pins "Harness version: 0.1.0" (also ships inside the published crate via `crates/do-harness/assets/compliance.md`) | `docs/compliance.md:3` | Refresh/derive at release | S |
| R6 | low | README/doc drift: hook command lacks `--record`; README `verify` row omits `--exclude`/`--jobs`/`--unchanged`/`--task`/`--global`; `docs/cli.md` example shows `catalog_size: 27` vs 13 real skills; `learn --draft` cites a CHANGELOG that does not exist; `metrics`/`scheduled` sets undocumented outside `do-harness.toml` | README, `docs/cli.md` | Sweep with R4 | S |
| R7 | med | Epic→heuristics citations unresolvable: `plans/mcp-conformance-epic.md` cites "harness heuristic 18/19/20/21/22" but `heuristics.md` bullets are unnumbered; harness trace IDs skip 5,13,31,32,38; newest ~25 entries cite PRs instead of trace IDs (the `trace add` discipline AGENTS.md §6 prescribes) | `plans/mcp-conformance-epic.md` | Cite file+trace-ID form; resume trace-ID recording on distillation | M |
| R8 | low | `pr-triage` SKILL.md presents semantic routing unconditionally (step 10, description), but the recorded verdict was **no-go** (+46.0% cost); fail-safe default keeps it safe, but a SKILL.md-only reader may configure a measured-worse router | `.agents/skills/pr-triage/SKILL.md`; `plans/pr-triage-skill-epic.md` Phases 5–7 | One clause marking it optional/experimental with the no-go citation | S |
| R9 | low | Duplicated 93-line `anti_ai_slop.md` in `skill-creator` and `skill-distiller` references; nothing enforces sync | both files | Single canonical copy + sync check or cross-reference | S |

Roadmap items confirmed current (no action): `chore-agt-promotion` hold correct
(agentmesh 4.0.0 still Public Preview, re-checked 2026-10-15); `chore-mcp-promotion`
hold decided with revisit triggers; `RUSTSEC-2024-0449` ignore stays until the SDK
drops `serde_yaml` (agentmesh 4.0.0 also depends on it 0.9.34);
`development-signals-epic` follow-ups all landed; `affected-selection-experiment`
closed final (`NO_GO`); DORA ratchet tolerates 3 breaches.

## A. Correctness (fix first)

| ID | Severity | Finding | Evidence | Action | Effort |
|----|----------|---------|----------|--------|--------|
| A1 | high | `doctor --format json` omits the event-chain tamper audit: the JSON report (with `"ok": failures.is_empty()`) is assembled and returned before `audit_chain` runs, which lives only in the text-mode branch — a tampered chain reports `ok: true` in JSON. All doctor tests use `Format::Text`, so the divergence is untested | `crates/do-harness/src/doctor.rs:117-140` vs `:205-221` | Hoist `audit_chain` before JSON assembly; add a chain-status field; JSON-mode tampered-chain test | S |
| A2 | high | WAL-blind migration backup: `backup_state_file` does `std::fs::copy` of the main DB file only, but the connection runs in WAL mode, so recent commits in `agent_state.db-wal` are omitted (or a torn state captured). This backs destructive migrations (0006 row deletion, 0011 table rebuild) | `crates/db/src/migrate.rs:267-275`; `connect` WAL at `:51-58` | `PRAGMA wal_checkpoint(TRUNCATE)` before copy, or `VACUUM INTO` | S/M |
| A3 | med | `advance_subtask_with_event` has no terminal-state guard inside its transaction: the UPDATE advances a `done`/`failed` task and appends `TaskAdvanced` after completion; only the CLI gate prevents it. Same class: `update_task_status_with_event` can re-complete a done task; `insert_task_with_event` stores caller-controlled `subtask_index` verbatim. The hash-chain audit still reports Intact (hashes, not transitions) | `crates/db/src/repo_workflow.rs:108-126`; `repo.rs:128-142`; CLI gate `task/mutations.rs` | Add status guards in-transaction (`WHERE ... AND status NOT IN (...)` + `InvalidTransition`); validate `subtask_index == 0` for pending inserts | S |
| A4 | med | The three eval writes are not atomic: latest + history + dim rows run back-to-back with no shared transaction — a crash leaves the read model claiming a run absent from history, and the bar ratchet / `metrics` compute from divergent data | `crates/db/src/repo_skill_eval.rs:29-54`, `repo_eval.rs:56-82,168-189`; caller `eval/orchestrator.rs:257-291` | One transactional writer `record_eval_run(latest, run, dims)` via `begin_immediate` | M |
| A5 | med | `bless_grader_baseline` uses a deferred transaction, violating the crate's own `tx.rs` invariant ("every write transaction should use `begin_immediate`") — deferred upgrade under two writers deadlocks with `SQLITE_BUSY` `busy_timeout` cannot resolve. Also `bump_error_signature`, `seed_invariants`, `record_sensor_outcome` lack the busy-retry wrapper their siblings carry | `crates/db/src/repo_eval/bars.rs:148`; `tx.rs:1-14`; `repo_exec.rs:229,292`, `repo.rs:177` | `begin_immediate` + uniform retry policy (or documented exceptions) | S |
| A6 | med | Migrations tracked by version only: an edited migration file after release diverges existing vs fresh databases silently, with no diagnostic | `crates/db/src/migrate.rs:174-199`, `migrate_catalog.rs` | Record SHA-256 of migration SQL at apply time; fail loudly on mismatch | M |
| A7 | high | reqwest follows redirects by default, defeating the SSRF guard: `upstream_client()` sets only `.timeout()`; a default policy of up to 10 hops means an allowed upstream answering `302 → http://169.254.169.254/` is followed silently — `validate_upstream` checks only the configured URL, never the redirect target. Breaks the "SSRF to cloud metadata … always rejected" claim on both egress paths | `crates/guardian-proxy/src/state.rs:145-152`; `server.rs:249`, `mcp/forward.rs:33-47`; `docs/threat-model-proxy.md` | `.redirect(reqwest::redirect::Policy::none())` + redirect-to-link-local test | S |
| A8 | med | IP-literal forms bypass the syntactic host checks: `::ffff:169.254.169.254` matches no prefix and passes the **always-deny** link-local rule; `::` and `::ffff:127.0.0.1` bypass the loopback opt-in; hex/octal IPv4 is checked raw but normalized at connect time | `crates/guardian-proxy/src/proxy.rs:222-250` | Parse host as `IpAddr`; use std range predicates, `to_ipv4_mapped`, `is_unspecified`; add a bypass test table | S/M |
| A9 | med | The deepseek-harness bundle's shipped default config is rejected by its own validator: `cordis.patch.yml` defaults `workspace: .` while `normalizeConfig` throws "workspace must be an absolute path"; every default profile mount fails at load, and no test exercises the default | `integrations/deepseek-harness/cordis.patch.yml:14`, `index.js:31-35` | Resolve relative `workspace` against `process.cwd()`, or change the default + add a default-config test | S |
| A10 | high | `pre-push = []` means the **entire 25-sensor suite** runs on every push — including `coverage` (the documented 11 GB llvm-cov OOM amplifier), `powerset`, `machete`, and the network `audit` — contradicting the "kept out of fast-loop hooks" comments on those sensors and the 2026-10-14 OOM postmortem | `do-harness.toml:5`; `config/mod.rs:41-42` "empty = full suite"; `hook_script.rs` appends no `--only`; `do-harness.toml:183,190` | Enumerate the push-gate sensors explicitly, or support a `@set` reference | S / M |

## B. LOC ceiling (500 hard cap, 450 warn)

| ID | Severity | Finding | Action | Effort |
|----|----------|---------|--------|--------|
| B1 | high | `src/commands.rs` 492 LOC; the `hook` dispatcher is a 157-line `#[allow(too_many_lines)]` fn | Extract `hook` + `print_first_difference` to `src/commands/hook.rs`, re-export like `commands/db.rs` | S |
| B2 | high | `src/doctor.rs` 486; trailing 204-line inline test module | `do-harness split` test extraction | S |
| B3 | high | `src/pr/waiver_comment.rs` 487; mixes model/parsing, evaluation, markdown rendering | Move the ~135-line rendering half to `src/pr/waiver_render.rs` | S |
| B4 | med | `src/main.rs` 466; `run()` is a 357-line dispatch monolith (~25 subcommands); Distill arm carries inline arg-validation `bail!`s | Group arms into `src/dispatch/`; move Distill validation into `distill.rs` | M |
| B5 | med | `src/cli.rs` 450; 8 more files at 425–449 (`config/tests.rs` 449, `dora/mod.rs` 448, `eval/eval_tests.rs` 447, `pr/readiness.rs` 439, `pr/command.rs` 437, `changes.rs` 437, `eval/orchestrator.rs` 435, `split.rs`/`sensor_inputs.rs` 425) | Pre-flight `do-harness loc --warn`; split in the same PR as feature work; cli.rs via `#[command(flatten)]` structs | S each / M cli.rs |

## C. Duplication & consolidation

| ID | Severity | Finding | Evidence | Effort |
|----|----------|---------|----------|--------|
| C1 | med | Two identical `percentile` impls; five unix-now blocks while `do_harness_db::unix_now` exists | `dora/mod.rs:190` vs `metrics/pr_loop.rs:261`; `verify.rs:33,389`, `events.rs:128`, `dora/mod.rs:173`, `metrics/pr_loop/window.rs:9` vs `db/src/migrate.rs:355` | S |
| C2 | med | Four independent `gh` runners with the same error shape; a duplicated check-run fetcher | `pr/gh.rs`, `pr/loop_fetch.rs`, `ci_explain/gh.rs`, `dora/gh.rs` | M |
| C3 | med | 24 `match format` sites handle serialization failure four ways (exit 0/1/2 varies by subcommand) | `report.rs:167`, `status.rs:318`, `explain.rs:97`, `init/report.rs:14`; `pr/command.rs:385`, `readiness.rs:351`, `metrics/pr_loop.rs:330`; `task.rs:69,83,285`; `version.rs:66` | S–M |
| C4 | med | Sensor verdict ladder duplicated between parallel and sequential paths ("must agree" by hand; orderings already differ) | `sensors/parallel.rs:67-99` vs `:135-143` | S |
| C5 | med | Exit-code classification of env/DB errors inconsistent (`doctor`/`eval` map to Verify; `task`/`trace`/`errors` to Usage) | `main.rs:408,414-416` vs `:317-322`; `eval/orchestrator.rs:95` | M |

## D/T. Test architecture & coverage

| ID | Severity | Finding | Evidence | Effort |
|----|----------|---------|----------|--------|
| D1 | med | 53 integration binaries; ~24 copies of `harness()`/`run()`/`git()`; three fake-`gh` routers; PR-1042 `view.json` pasted 7× | `tests/` | M |
| D2 | med | Missing e2e: `hook diff`, `maintenance --prune-beats`, `overlap`, `doctor --format json` (A1 is this gap's product) | `tests/` | M |
| T1 | high | Coverage gate never runs on PRs (`coverage` is in the `release` set only) — regressions land silently on main | `do-harness.toml:9-10`; `verify.yml:162` vs `release.yml:111` | S |
| T2 | high | The 70% number includes test code: llvm-cov runs with no exclusion regex; ~20,200 of ~39,400 do-harness LOC are tests that self-report as covered | `scripts/check-coverage.sh:158` | M — *note (batch 17–18):* not reproducible as stated — the report already carries no test-*file* rows (142 rows, 0 matching); the residual is 91 `src` files with inline `#[cfg(test)]`, which a path filter cannot reach |
| T3 | high | DORA `--source gh` enrichment effectively untested (3 inline tests, no fake-`gh`; tag/release reconciliation has zero coverage) — and the path touches the network | `dora/gh.rs:70,129,183,223` | M |
| T4 | high | Tautology-adjacent: v3/v4 evidence compatibility test accepts exit 0 **or** 1; the only v3-readability guard | `tests/release_contract.rs:256-258,266` | S |
| T5 | high | `metrics` skill-trends aggregation (~70 LOC lift/dim/bar joins) has zero assertions | `metrics.rs:161-227`; `beat_scope.rs:90-145` | M |
| T6 | med | 47 `#[cfg(unix)]` tests incl. the entire `pr ready` suite — merge-gating logic has no coverage on a supported Windows platform, and the skip is silent | `support/pr_readiness.rs:126-130`; `verify.yml:393` | M |
| T7 | med | `test` sensor `--no-tests=pass` with no minimum-count guard (a suite shrunk to zero still passes; doctest has a can-fail proof, `test` has none) | `do-harness.toml:36`; `check-coverage.sh:62-145` | S |
| T8 | med | `ci_parity.rs` four silent `return;` skip paths — fail-closed contract unverified when a tool is on PATH | `ci_parity.rs:45-46,64-65,165,189` | S |
| T9 | med | eval `db:` assertion graded only on its failing path (broken query always returns empty) | `eval/eval_tests.rs:198-207` | S — *note (batch 8):* the passing path *is* covered at the grader level; the runner level grades inside the hermetic sandbox, so the contract to pin is that caller state must not satisfy a `db:` assertion |
| T10 | med | `.codecov.yml` is dead config: no workflow uploads; the 60% patch floor is unenforced | `.codecov.yml` | S |
| T11 | med | `tests/selection_impact_test.sh` defaults to a possibly-stale release binary and mutates a committed fixture in place | `selection_impact_test.sh:11,203-240` | S |
| T12 | med | nested-build-isolation Rule 3 (profraw litter check) is manual only | skill vs workflows | S |
| T13 | med | Slow-path concentration: dogfood/init tests each pay a nested cargo cycle (fed the coverage OOM) | `dogfood.rs`, `init_bootstrap.rs`, `coverage_sensor.rs` | M |
| T14 | med | Pre-push hook asserted by string-match only, never executed end-to-end | `hooks/tests.rs:52-56` | M |
| T15 | med | Benchmark fixtures unreferenced; `pr-routing-large` has no regeneration drift check | `tests/fixtures/`; `.github/` grep | S |
| T16 | low | Exactly one doctest in the whole workspace | `types/src/chain.rs:36-43` | S |
| T17 | low | nextest `[profile.ci]` is dead config (nothing sets `NEXTEST_PROFILE`) | `.config/nextest.toml:12-13` | S |
| T18 | low | `db` crate `repo_metrics.rs`/`repo_sensors.rs` have no direct unit tests | `crates/db/src/` | S |

## E. Performance & hygiene

| ID | Severity | Finding | Effort |
|----|----------|---------|--------|
| E1 | low | Blocking `std::process`/`std::fs` on the current-thread runtime — correct today; guard with `spawn_blocking` + `Semaphore` only if `metrics pr` parallelizes | S |
| E2 | low | `pr/readiness.rs` re-runs the whole gh collection up to 3×; `user_permission` uncached | S |
| E3 | low | Dead payload fields + triple re-export chains in `pr/` | S |
| E4 | low | Duplicated root-resolution logic and verbatim hint strings | S |

## S. Schema/type contracts (types + db)

| ID | Severity | Finding | Evidence | Effort |
|----|----------|---------|----------|--------|
| S1 | med | The `Command` half of the event-sourcing contract is dead in production: exported and tested, zero call sites; the real writers reimplement it with different types and can drift (`AddTask.precondition`/`parent_id` never reach `TaskAdded`) | `types/src/event.rs:12-15`, `workflow.rs:16-73` | M |
| S2 | med | Beat `status`/`beat_type` are free-form strings that gates match as exact literals across two crates; a typo silently fails gates. DB CHECK constrains only `sensor_name` | `types/src/beat.rs:21-23`, `repo_exec.rs:17-19`, `repo_workflow.rs:192`, `repo_metrics.rs:32`, `telemetry.rs:158-164` | M |
| S3 | med | Scope-key contract duplicated four ways, unenforced; nothing validates `beats.task_id` vs `beats.scope`, so the two gate lookups can disagree | `repo_exec.rs:36-46,74-87`; `telemetry.rs:41-92` | M |
| S4 | med | No batched struck-signature query: one query per sensor per verify run | `repo_exec.rs:396-420` | S |
| S5 | med | `repo_sensors.rs` untested in-crate (ratchet monotonicity, `previous_max`); same for `has_ok_beat`, `vacuum`, orphan detector, paging limits | `crates/db/src/repo_sensors.rs` | M |
| S6 | med-lo | `Precondition` is prose-only; the "typed HTN" cannot evaluate its own guards | `types/src/htn.rs:43-48` | S/M |
| S7 | low | Three inconsistent string-parsing idioms across wire enums | `htn.rs:79-93` vs `eval_dim.rs:68-84`, `eval_run.rs:37-49` | S |
| S8 | low | `parse_invariants_json` returns `Result<_, String>` (only untyped error in the crate) | `types/src/decision.rs:61-74` | S |
| S9 | low | `DbError::Constraint(String)` discards the source code/chain (blocks matching constraint kinds) | `db/src/error.rs:67-69` | S |
| S10 | low | `NotFound` doubles as the "impossible branch" error | `repo.rs:53,115,140`, `repo_exec.rs:68,270`, `repo_dora.rs:135`, `repo_skill_eval.rs:49` | S |
| S11 | low | Seq-retry loop retries every `Constraint`, not just the `UNIQUE(seq)` race | `repo_workflow.rs:293-329` | S |
| S12 | low | `input_digest` bolted on with a post-insert `UPDATE` | `repo_exec.rs:304-316,341-348` | S |
| S13 | low | Magic `limit = -1` sentinel across four paging APIs | `repo_exec.rs:103`, `repo_eval.rs:102`, `repo_trace.rs:66`, `repo_dora.rs:144` | S |
| S14 | low | `latest_sensor_beat` returns an anonymous 4-tuple | `repo_exec.rs:372-389` | S |
| S15 | low | Serde-contract tests for only 2 of 15 structs | `types/src/task.rs:39-45` | M |
| S16 | low | No index on `heuristics(skill_name)` | `migrations/0001_init.sql:70-74` | S |
| S17 | low | `pass_rate`/`without_pass_rate` bare `f64` (NaN poisons ratchet ordering) | `eval_run.rs:70-74`, `skill_eval.rs:22` | S/M |
| S18 | low | Unknown stored eval mode silently degrades to `Deterministic` | `repo_eval.rs:127-141` | S |
| S19 | low | `tx.rs` serialization test does not exercise the claimed contention | `tx.rs:42-75` | S |

## F. Supply chain & dependencies

| ID | Severity | Finding | Action | Effort |
|----|----------|---------|--------|--------|
| F1 | med | The vendored libSQL line (0.9.30) faces upstream soundness/durability issues (#2285 `Row::get_value` panic on malformed UTF-8 in 0.9.30; #2271 bundled SQLite predates the 3.51.3 WAL-reset fix; #2254 power-loss corruption; #2272 authorizer heap UAF). Upstream 0.10.0 shipped 2026-10-13 | Add an upstream-issue watch (mirror `check-agt-ga.sh`); record affected issues in `plans/libsql-distribution.md`; schedule the 0.10.0 rebase decision | M |
| F2 | med | `cargo-semver-checks` exists but runs **only on tag push** (release preflight); a PR breaking a published crate's API merges green and surfaces at tag time. Not a sensor | Add a `semver` sensor to a `release-prep` set or run on `push: main` (needs registry access) | M |
| F3 | med | rmcp 3.x declares MSRV 1.88 vs the workspace 1.85 (already recorded in `plans/mcp-conformance-epic.md`); newest rmcp is 3.5.1 (pin `=3.5.0`). *Note (batch 9):* the adjacent `.clippy.toml` gap is already closed — the file declares `msrv = "1.85"`, matching `Cargo.toml`'s `rust-version`, so clippy is already MSRV-aware | Bump rmcp deliberately | S |
| F4 | low | Advisory ignores lack expiry dates and use the bare-string form while `[bans] skip` uses `{ crate, reason }` | Add expiry + reason form | S |

## P. Sensor policy, scripts & CI

| ID | Severity | Finding | Evidence | Effort |
|----|----------|---------|----------|--------|
| P1 | med | Guardian-proxy tests run twice per CI job (raw step + sensors); the raw step bypasses `when-changed` | `verify.yml:153` vs `do-harness.toml:96,101` | S |
| P2 | med | Toolchain/clippy/nextest configs missing from the Rust sensors' `when-changed` (only `fmt` lists the toolchain) | `do-harness.toml:22` vs `:27,32,37,42,48` | S |
| P3 | med | Sensors whose own scripts hold their policy are not in their `when-changed` (`audit` omits `check-audit.sh`, `loc`/`coverage` likewise); `deps` runs the python validators undeclared | `do-harness.toml:53,48,73` | S/M |
| P4 | med | The `scheduled` set runs on every push/PR (no `schedule:` trigger); `powerset`'s four builds paid per PR; its evidence neither asserted nor uploaded | `do-harness.toml:17`; `verify.yml:165-169,222-230` | S |
| P5 | med | Two CI-executed shell scripts escape the `shell` sensor and shellcheck (`-maxdepth 1`; fixtures scoping) | `scripts/check-shell.sh:31-32` | S |
| P6 | med | Vacuous-pass gaps in shipped sensors (against the repo's own No-Vacuous-Gates rule): markdownlint/yamllint exit 0 on zero files *(fixed)*; `check-privacy.sh` treats unreadable files as non-findings *(open)*; `check-untrusted-ingest.sh` SKIPs before its contract grep *(fixed)* | `check-markdownlint.sh:41-44`, `check-yamllint.sh:35-38`, `check-privacy.sh:66-76`, `check-untrusted-ingest.sh:38-41` | S |
| P7 | med | No `actionlint` on workflow files (yamllint checks style only) | `.github/workflows/*.yml` | S |
| P8 | med | No repository-wide markdown link validation | `check-skills-docs.sh:64-77` | S/M |
| P9 | med | `source-distribution` job has no rust-cache; Playwright Chromium re-downloaded every run | `verify.yml:26-74,130-134` | S |
| P10 | med | Windows CI never runs this repo's own sensor suite | `verify.yml:370-416` | M |
| P11 | low | `feedback` set omits `loc` while the pre-commit hook runs it; `commitlint`'s always-applicable choice undocumented | `do-harness.toml:8,4,129-130` | S |
| P12 | low | `require_tools()`/`ROOT` duplicated across 8+ scripts; three sha256 helpers; the workspace-version `sed` parse duplicated 7×; `check-dora.sh` re-implements `hook_script.rs` binary resolution | scripts | M |
| P13 | low | CI nits: no `workflow_dispatch` on verify; peripheral workflows lack concurrency; unused `cargo-llvm-cov` in verify (confirmed by a CI run without it); hardcoded toolchain in the benchmark workflow; `[profile.ci]` unused; unpinned `install-action` tool versions; `contributor-check.yml` interpolates action output into `run:` | `verify.yml`, `Cargo.toml`, `selection-impact-benchmark.yml`, `contributor-check.yml` | S |
| P14 | low | `.pre-commit-config.yaml` duplicates the managed hook and hardcodes `./target/release/do-harness` (the `CARGO_TARGET_DIR` staleness trap); never run in CI | `.pre-commit-config.yaml:8` | S |
| P15 | low | `check-artifact-provenance.sh`/`check-rust-binary-provenance.sh` tested but unwired; the release job could self-check `dist/*` against its own `checksums.txt` | grep: only test files exercise them | S |
| P16 | low | Script polish: unguarded `$2` under `set -u`; FAIL/SKIP lines without hints; `check-loc.sh` suppresses find errors; markdown/yaml lint tracked files only; inconsistent python3-availability policy | assorted scripts | S |

## Followups (priority order)

1. **R1** — rescue the stranded `v0.3.0` release: configure Trusted Publishing
   for `do-harness-libsql`, re-cut the tag on `787c19f`, verify completion.
2. **A10** — one-line `pre-push` config fix (removes the OOM-amplifier footgun
   that contradicts the sensor comments).
3. **A7** — reqwest redirect policy (closes an SSRF bypass of a documented
   threat-model claim); then **A1** (doctor JSON audit) and **A2** (WAL-blind
   backup) as standalone correctness fixes.
4. **A3–A5** as one db-invariants package; **S2–S4** as one beat-vocabulary
   package (`BeatStatus`/`BeatType`/`Scope` in `crates/types`).
5. **T1+T2+T7** as one coverage-signal PR; **T3+T5** as the two highest-value new
   tests; **P6** as one vacuous-gate hardening pass.
6. Roadmap/doc sync (**R2/R4/R5/R6**) once the release lands; LOC splits
   (**B1–B3**) opportunistic; **F1/F2/P2/P3** as policy/supply-chain chores.

*Landed (2026-10-15):* A1, A2, A3, A4, A5, A7, A8, C1, C4, C5, F5, P1, P2, P3,
P4, P5, P6 (partial), P9, P11, P13, P14, P16 (partial), R2, R3, R4, R5, R6, R7, R8, R9,
S11, S12, S15, S16, S17, S18, S19, T1, T2, T3, T4, T5, T7, T8, T9, T10, T11, T12, T14,
T15, T16, T17, T18,
B2 (forced by A1).
*Next:* R1 (operator-gated), A10 (maintainer decision), A6, C2, C3, R2, R3,
R7.

*Harness addition (2026-10-16):* section 5's "named constants over magic numbers"
is now executable rather than prose. `scripts/check-magic-numbers.sh` (with
`scripts/check_magic_numbers.py`) reports every numeric literal that is not a
named constant and not one of `0`/`1`/`2`, ignoring comments, string/char/raw
literals, and attribute spans; it is registered as the warn-severity
`magic-numbers` sensor in the `verification` set, with a blessed findings
baseline (`plans/baselines.json`: 1724) so existing occurrences are grandfathered
and any *increase* is a hard regression. Proven end to end: adding one literal
fails with `ratchet regression: findings 1725 exceed blessed baseline 1724`, and
the session's own new files were named before the first bless (dora_gh, pr_routing,
maintenance), with the two reporting test files' fixture matrices left to the
baseline on purpose — that is what the ratchet is for. An invariant in
`plans/invariants.json` (51 headers) carries the rule with this sensor.
R7.

## Verification notes

- No finding was accepted without a source spot-check for every HIGH item
  (doctor JSON, commands.rs LOC, WAL backup, `begin_immediate`, reqwest redirect,
  IP-literal checks, `pre-push` semantics, DSH default config, coverage flags,
  `cfg(unix)` count, crates.io publication record).
- F2 was corrected during verification: `cargo-semver-checks` already runs in the
  release preflight — the gap is PR-time visibility, not absence.
- Sensor/config findings (`A10`, `P*`) change `do-harness.toml` or
  `scripts/*.sh`; re-run `do-harness verify --set verification --strict` and the
  affected sensor in a dev checkout after any change.
- Persistence findings (`A2`–`A6`, `S*`) touch the state DB contract; write a
  regression test at the db layer, not only the CLI.

## References

- <https://crates.io/api/v1/crates/do-harness-libsql> (publication record, `trustpub_data: null`)
- <https://github.com/modelcontextprotocol/rust-sdk> (rmcp 3.5.1, MSRV 1.88)
- <https://tokio.rs/blog/2025-01-01-announcing-axum-0-8-0> (axum 0.8 path syntax)
- <https://github.com/tursodatabase/libsql/issues> (#2254/#2271/#2272/#2285)
- <https://github.com/obi1kenobi/cargo-semver-checks>
- <https://github.com/EmbarkStudios/cargo-deny> and rustsec/cargo-audit advisories workflow
