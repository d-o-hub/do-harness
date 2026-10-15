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
S9–S18, T1–T18, F1–F4, P1, P4, P7–P16, B1, B3–B5, C2, C3, C5, D1–D2, R2, R3,
R7.

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
| T2 | high | The 70% number includes test code: llvm-cov runs with no exclusion regex; ~20,200 of ~39,400 do-harness LOC are tests that self-report as covered | `scripts/check-coverage.sh:158` | M |
| T3 | high | DORA `--source gh` enrichment effectively untested (3 inline tests, no fake-`gh`; tag/release reconciliation has zero coverage) — and the path touches the network | `dora/gh.rs:70,129,183,223` | M |
| T4 | high | Tautology-adjacent: v3/v4 evidence compatibility test accepts exit 0 **or** 1; the only v3-readability guard | `tests/release_contract.rs:256-258,266` | S |
| T5 | high | `metrics` skill-trends aggregation (~70 LOC lift/dim/bar joins) has zero assertions | `metrics.rs:161-227`; `beat_scope.rs:90-145` | M |
| T6 | med | 47 `#[cfg(unix)]` tests incl. the entire `pr ready` suite — merge-gating logic has no coverage on a supported Windows platform, and the skip is silent | `support/pr_readiness.rs:126-130`; `verify.yml:393` | M |
| T7 | med | `test` sensor `--no-tests=pass` with no minimum-count guard (a suite shrunk to zero still passes; doctest has a can-fail proof, `test` has none) | `do-harness.toml:36`; `check-coverage.sh:62-145` | S |
| T8 | med | `ci_parity.rs` four silent `return;` skip paths — fail-closed contract unverified when a tool is on PATH | `ci_parity.rs:45-46,64-65,165,189` | S |
| T9 | med | eval `db:` assertion graded only on its failing path (broken query always returns empty) | `eval/eval_tests.rs:198-207` | S |
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
| F3 | med | rmcp 3.x declares MSRV 1.88 vs the workspace 1.85 (already recorded in `plans/mcp-conformance-epic.md`); newest rmcp is 3.5.1 (pin `=3.5.0`). Adjacent new gap: `.clippy.toml` declares no `msrv = "1.85"`, so clippy may suggest post-1.85 idioms that break the MSRV job | Bump rmcp deliberately; add `msrv = "1.85"` to `.clippy.toml` | S |
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
| P13 | low | CI nits: no `workflow_dispatch` on verify; peripheral workflows lack concurrency; unused `cargo-llvm-cov` in verify; hardcoded toolchain in the benchmark workflow; `[profile.ci]` unused; unpinned `install-action` tool versions; `contributor-check.yml` interpolates action output into `run:` | `verify.yml`, `Cargo.toml`, `selection-impact-benchmark.yml`, `contributor-check.yml` | S |
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

*Landed (2026-10-15):* A1, A2, A3, A4, A5, A7, A8, C1, C4, F5, P2, P3, P5,
P6 (partial), R4, R5, R6, R8, R9, S4, S5, S8, S19, T18, B2 (forced by A1).
*Next:* R1 (operator-gated), A10 (maintainer decision), A6, C2, C3, C5, R2, R3,
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
