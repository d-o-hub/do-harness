# AGENTS.md — Autonomous & Interactive Agent Execution Harness

## 1. System Operating Invariants (Hard Constraints)
- **Modularity Cap**: Maximum 500 Lines of Code (LOC) per source file. Decompose when nearing 450 LOC.
- **Persistence Engine**: Local libSQL instance (`.do-harness/agent_state.db`) for tracking beats, task execution traces, and learned heuristics. The database is local state and is never committed (`.do-harness/` is ignored): a live SQLite file has no diff or merge, carries `-wal`/`-shm` sidecars, and a copy taken while a transaction is active can be corrupt. Version the contract instead — `crates/db/migrations/*.sql`, `plans/*.json`, and `task export` snapshots (see `docs/retention.md`).
- **Persistence & Roadmap**: The `do-harness-db` repository layer implements persistence writers for tasks, beats, traces, heuristics, error signatures, and skill evals (`verify --record` persists beats scoped to `--task` or git branch, resetting strikes on pass; `task list`/`task export` read task state). The workflow runtime (`task add/advance/done/fail`, gated on `plans/methods.json` and named sensor ok beats, with typed Commands/Events and `TaskBoard` projections), fail-fast recovery (`errors list/clear`, strike counters scoped per workstream), the sandboxed skill-eval runner (`do-harness eval` with blessed tamper-evident baselines and `eval --agent-cmd` lift measurement), the `metrics` command, the deterministic DORA collector (`dora`), and commit enforcement (`check-commitlint.sh`) are implemented. The development-signal contract (`[signal-sets]` + `verify --set`, `when-changed` + `verify --changed`/`explain`, workspace/policy fingerprints, evidence schema v4, and status reporting) is documented in `plans/development-signals-epic.md`. Architecture policies and sensor decisions live in `plans/invariants.json`. Distilled operational heuristics reside in `.agents/skills/harness/references/heuristics.md`. Release distribution (multi-platform static binaries, cargo-binstall metadata, zero-install npm platform packages, automated SemVer release proposals via `scripts/release-manager.sh` and `.github/workflows/auto-release.yml`, and post-publication verification) is documented in `docs/releasing.md` and `plans/distribution-epic.md`. MCP conformance and governance policies are tracked in `plans/mcp-conformance-epic.md` and `plans/agt-governance-epic.md`.
- **Verification Priority**: Computational sensors (`cargo test`, `cargo check`, linters) strictly supersede LLM self-assessment.
- **Workspace Cleanliness**: All skills reside in `.agents/skills/`. Task tracking resides in `plans/`. Architecture rules are **executable invariants**, not prose ADRs (see §7).

---

## 2. Hierarchical Execution Pipeline (HTN + Event Slices)

```
[ Objective / User Request ]
             │
             ▼
[ 1. HTN Task Decomposition (.agents/skills/htn-planner) ]
             │
             ├── Requires Uncertainty Spike? ──> [.agents/skills/spike-runner]
             │                                          │ (libSQL trace)
             │                                          ▼
             └──> [ 2. Event Model Schema Definition (.agents/skills/event-modeler) ]
                               │
                               ▼
                  [ 3. ATDD Red Phase: Write Failing Fixtures ]
                               │
                               ▼
                  [ 4. TDD Green Phase: Minimal Implementation ]
                               │
                               ▼
                  [ 5. Computational Harness Verification (Sensors) ]
                               │
                               ▼
                  [ 6. Dreaming & Skill Distillation (.agents/skills/skill-distiller) ]
```

---

## 3. The 6-Phase Coding Workflow

### Phase 1: HTN Planning & Decomposition
- Break compound requests into primitive tasks stored in `plans/tasks.json` or `.do-harness/agent_state.db`.
- Check preconditions before invoking any action.

### Phase 2: Spike Execution (If Ambiguity Exists)
- If third-party APIs, performance boundaries, or novel crate integrations are uncertain, isolate them in `tests/spikes/` or a temporary branch.
- Record findings directly to the local database and clean up scratch code.

### Phase 3: Event Modeling (Given-When-Then Contracts)
- Split domain behavior into two discrete slice types:
  1. **State Change**: `Command -> Handler -> Event(s)`
  2. **State View**: `Event(s) -> Projection / Read Model -> Query`
- Define Rust structs, enums, and typed errors before implementation.

### Phase 4: Acceptance Test-Driven Development (ATDD)
- Author end-to-end integration fixtures verifying:
  - `Given`: Initial event history loaded into libSQL/memory.
  - `When`: Command is dispatched.
  - `Then`: Specific domain events emitted and read models updated.
- Verify that the test fails for the expected reason (`Red`).

### Phase 5: Implementation & Computational Sensors (Green & Refactor)
- Implement the minimal handler logic to satisfy the test (`Green`).
- Run the full verification suite via the unified entrypoint (`do-harness verify` runs every configured sensor — see `[[sensors]]` in `do-harness.toml`):
  <!-- example: id=agents_verify classification=illustrative reason="Self-correction verify command" -->
  ```bash
  do-harness verify
  ```
- For targeted re-runs after a specific sensor fires (self-correction protocol), invoke the individual command:
  <!-- example: id=agents_targeted_reruns classification=illustrative reason="Targeted cargo re-runs" -->
  ```bash
  cargo check
  cargo test
  cargo clippy -- -D warnings
  ```
- Refactor if necessary while respecting the 500 LOC ceiling.

### Phase 6: Dreaming & Skill Distillation Loop
- Extract successful diffs and error-recovery patterns from the interaction trace.
- Update matching skills in `.agents/skills/` or create new evaluated skills.
- Run skill evaluation fixtures to benchmark token efficiency and test pass rates.

---

## 4. CLI Tool Protocol & Guardrails
- **File Edits**: Inspect AST / types before rewriting files.
- **Fail-Fast Policy**: If a computational sensor fails 3 consecutive times on the same subtask, halt, record the error signature in libSQL, and surface a diagnostic to the developer.
- **No Hallucinated Success**: A subtask is complete only when verified by automated exit codes.
- **No Vacuous Gates**: a green sensor that executed zero checks is a failed gate, not evidence. Treat `0 tests run` or empty sensor output as RED: add a real check (e.g. a doc-test on a public API) or cite the committed can-fail proof for that sensor (the `dogfood.rs` broken-doctest fixture is the model for `doctest`).
- **Hooks**: `do-harness hook install` writes .git/hooks/pre-commit (fmt + loc, run with `--record` so beats persist), commit-msg (commitlint), and pre-push (full verify); uninstall/status remove/inspect managed hooks. Hooks resolve the binary from `DO_HARNESS_BIN`, PATH, or `target/release/do-harness` (build it or set the env var in dev). When `CARGO_TARGET_DIR` redirects builds elsewhere, `target/release/do-harness` goes stale silently: the binary then predates the schema (fail-closed `doctor`/`init-db --check` skew) and hooks verify old code. In such checkouts always export `DO_HARNESS_BIN=$CARGO_TARGET_DIR/release/do-harness` (after `cargo build --release -p do-harness`) for every verify, hook, and commit invocation, and treat the `is older than workspace sources` warning as a hard stop, never background noise. On memory-constrained machines also export `CARGO_BUILD_JOBS=2` and `CARGO_INCREMENTAL=0` (the CI setting in `verify.yml`) before any verify or push: an uncapped full verify invoked the host OOM killer on 2026-10-14.
- **Workflow gates**: pre-commit = `do-harness verify --fail-fast --only fmt --only loc`; pre-push = `do-harness verify --fail-fast`; CI = `verify.yml` lints shell + `cargo check -p guardian-proxy` (default features; the feature-on suites run through the `check-agt`/`check-mcp` sensors) + `do-harness verify --set verification --format json --strict` (exit 0/1/2) + `do-harness status --set verification` (must be green) + `verify --set scheduled` + dogfood `init && verify` + `do-harness eval --strict-fixtures`, alongside `source-distribution`, `msrv`, `dsh-bundle`, and `windows` jobs. The heavy `release` preflight and `scheduled` jobs add a 4 GB swap file and run `coverage` with `CARGO_BUILD_JOBS=1` and `NEXTEST_TEST_THREADS=1`: the instrumented suite is the documented ~11 GB OOM amplifier, and a hosted runner starved under memory pressure reports only exit 143 plus a shutdown signal. Their runner is pinned to `ubuntu-24.04` because `ubuntu-latest` migrates to 26.04 from 2026-10-19.
- **Workflow YAML guardrail**: keep `.github/**/*.yml` and `.github/**/*.yaml` in the repository's yamllint style, using block mappings for workflow environment and configuration tables. Do not replace them with compact `{ ... }` mappings to save lines: `yamllint` rejects spaces inside flow braces here. After any workflow/config edit, run `bash scripts/check-yamllint.sh` and `bash scripts/check-actionlint.sh` (or `do-harness verify --only yamllint --only actionlint`) before broader verification; formatting sensors take precedence over line-count compression.
- **Immutable action references**: every external action and reusable workflow in `.github/workflows/**` must reference a full 40-hexadecimal commit SHA, and every Docker action image must carry a `sha256:` digest; a tag, a branch, a short SHA, or an expression is rejected with the file and line. `actionlint` proves the workflow is valid, `action-pins` proves its dependencies cannot move under you. Run `bash scripts/check-action-pins.sh` (or `do-harness verify --only action-pins`) after any workflow edit. The check is offline — it never calls the GitHub API and never resolves a tag — so it proves the shape of a reference, not that the commit exists.
  Source: [GitHub's security hardening guidance](https://docs.github.com/en/actions/security-for-github-actions/security-guides/security-hardening-for-github-actions).
- **Documentation contract**: after any change to the CLI surface or a maintained document, regenerate the mechanical blocks (`python3 scripts/docs.py sync --write`), inspect that diff, run the three docs checks (`python3 scripts/docs.py sync --check`, `python3 scripts/docs.py examples --check`, `python3 scripts/docs.py links --check`), then review the affected narrative by hand — the checks prove the mechanical facts, never the prose; scope, exemptions, and recovery live in `docs/documentation-contract.md`.
- **PR lifecycle**: after opening a PR, load `.agents/skills/pr-triage` and run the sweep without waiting to be asked (review/roast the diff, resolve threads, wait for every check, merge with `gh pr merge --squash --match-head-commit <validated head>`; never auto-merge; a draft, `hold`/`wip`/`do-not-merge` label, or "do not merge" remark is a hard stop — report it, never `gh pr ready` a pull request to make a merge possible, and never merge a draft without explicit human approval recorded in the session, which `.agents/skills/pr-triage/scripts/premerge.sh` enforces immediately before the merge). In a dev checkout set `DO_HARNESS_BIN=target/release/do-harness` so the skill uses `do-harness pr review` instead of falling back to the full diff.
- **Persistence commands**: `do-harness task list`/`task export` surface task state (libSQL is the source of truth; export writes `plans/tasks.json` with a board summary and `task import --check` validates freshness — import is validation-only by design, since ids are database-assigned and the event log is append-only, so a snapshot cannot be replayed back into the database); `verify --record` persists beats (scoped to the current git branch by default, to `--task <id>` when passed, or `--global`), and bumps error signatures for failing sensors; per-sensor `severity = "error"|"warn"` (with `allow_failure` as the deprecated warn alias) makes warn failures advisory except under `--strict` outside `feedback`, and warn strikes quarantine after 3; `FINDINGS: <n>` markers ratchet against the committed `plans/baselines.json`, which `verify --record --bless` only lowers (raise refused, bless history in libSQL); `task add/advance/fail`, `trace add/list`, `distill`, `eval`, `metrics`, and `maintenance --prune-beats` are implemented. `plans/methods.json` ships `vertical-event-slice`, `spike-and-resolve`, and `decision` (a promotion verdict gated on the decision-agnostic `check-promotions`). The global `--dry-run` covers every mutating `task` action including `add` and `export`; `task remove` refuses a task that has workflow events rather than deleting hash-chained audit rows.
- **Signal commands**: `verify --set <set>` runs one development decision's sensors (writing `.do-harness/evidence.<set>.json`); `verify --changed` / `explain --set <set> --changed` apply deterministic `when-changed` selection (fail-closed when git state is unreadable); `status --set <set>` reports evidence freshness `green|red|stale|missing` without running sensors; `list --sets` enumerates configured sets.
- **LOC feedforward**: the 500-LOC ceiling is enforced by the `loc` sensor (`scripts/check-loc.sh`), which on failure prints the largest top-level item spans (`SPAN:`) and the code/test split (`CODE:`/`TEST:`) and always reports a `FINDINGS: <n>` over-limit count for the blessed ratchet. `--root`/`--ext` widen the scan beyond default workspace language roots (generated trees are pruned), so a front-end tree gets the same ceiling from `do-harness.toml` without forking the script. `do-harness loc [--dir <dir>] [--ext <ext>] [--warn] [--format json] [paths…]` answers the same question without running the sensor, automatically discovering workspace language roots (`packages/`, `apps/`, `src/`, `crates/` for `generic`/`node`/`web`), and `do-harness split <file> [--dry-run] [--target <name>]` performs the decomposition: it extracts the inline `#[cfg(test)]` module to `<stem>_tests.rs` (declared with `#[path = …]`), or the largest top-level item that both fits the ceiling and brings the source under it, and refuses with a diagnostic rather than emitting code that cannot compile (`macro_rules!`, `include!`, non-test `#[cfg]`).
- **Sensor policy decisions**: explicit wontfix entries (gitleaks, SBOM, lockfile freshness, eval bless-drift) live in `plans/invariants.json`; `do-harness seed --prune` keeps libSQL in sync and drops stale rows. `shell` (shellcheck, fail-closed in CI), `actionlint` (GitHub Actions workflow semantics; tool-dependent so it SKIPs locally and fails closed under `CI=true`/`DO_HARNESS_REQUIRE_TOOLS=1`, mirroring shellcheck), `action-pins` (every external action and reusable workflow must reference a full 40-hexadecimal commit SHA and every Docker action image a `sha256:` digest; the parser is PyYAML, so a missing parser SKIPs locally and fails closed under `CI=true`/`DO_HARNESS_REQUIRE_TOOLS=1`, mirroring the actionlint policy), `docs-sync` (the generated regions of `README.md` and `docs/cli.md`, plus `tests/fixtures/cli-contract.json`, must render byte-for-byte from the Clap definitions; repair with `python3 scripts/docs.py sync --write`, never by hand-editing a region), `docs-examples` (every `<!-- example: ... -->` block in a maintained document executes against its manifest fixture, or parses as its classification declares; an unmarked CLI/config fence fails), `docs-links` (maintained-document link and anchor existence, inventoried by `docs/contracts.toml`, which is what exempts historical, fixture, and vendored documents; fails closed on an empty inventory), `check-agt` (guardian-proxy governance feature), `check-mcp` (guardian-proxy MCP surface feature), `powerset` (guardian-proxy feature-powerset check), `machete` (unused-dependency pruning), `magic-numbers` (section 5's named-constants rule, warn severity with a blessed findings baseline so only an *increase* fails), `untrusted-ingest` (the `pr-triage` skill is the only component whose input is attacker-controlled by design: its ingest scripts must hold no dynamic-execution sink — `eval`, `sh -c`, `child_process.exec`, shell-enabled `spawn` — and its untrusted-data contract clause must still be stated; a sink lint, not a taint tracker, so it proves the dangerous sinks are absent rather than exhaustively proving no path exists), and `check-promotions` (every recorded `*-promotion` verdict must agree with the shipped Cargo defaults — a coherence gate only; it does not re-derive GA, because the GA watch exits 0 on every verdict and needs the network, so the dated verdict stays recorded epic evidence) are workspace sensors.
- **Release derivation**: `scripts/release-manager.sh` is the one release entry point. `status` and `next [--format json]` derive the bump from the conventional commits since the last release tag, `prepare <version> [--no-lock] [--no-man]` applies it to every managed manifest (`[workspace.package].version`, the intra-workspace requirement pins in the member manifests, `Cargo.lock`, the six npm manifests, `man/do-harness.1`), and `--self-test` pins the rules and the fail-closed guards as the `release-manager` sensor; the release-gated `release-manager-e2e` sensor runs the opt-in `--e2e` (the real `prepare` in a scratch `git worktree`, asserting `cargo metadata --locked` after the bump) and the hermetic `publish-npm` sensor runs `scripts/publish-npm.sh --self-test`, neither of them in `verification`. It validates a version as plain `MAJOR.MINOR.PATCH` before it can reach `sed` (pre-release and build metadata are refused), validates the git ref before use, and fails closed on a `rev-list`/`git log` error instead of deriving `bump=none` from a broken range. The bump policy belongs to `next_version`: breaking takes the major at ≥1.0 and the minor at 0.x, `feat` the minor at ≥1.0 and a patch at 0.x, `fix`/`perf` a patch, a non-conventional subject a patch, nothing else releases, and `0.0.x` stays on patches whatever the commits. It resolves its root from its own location, so invoking another checkout's copy operates on *that* tree: pass `--root <dir>` when you mean a different one. Tagging is deliberately not part of it — `main` requires pull requests, so the bump lands as a reviewed PR and `.github/workflows/auto-release.yml` refreshes that PR (branch and body, retiring any superseded automation-owned `release/v*` proposal) and tags it after the merge, and only when `github.ref == 'refs/heads/main'`; the release is complete only once its GitHub Release exists — `ship` tags the commit, and crates.io, npm and the Release are created afterwards by `release.yml`, so a tag is intent, not a finished release — and a basis tag with no published Release derives `bump=pending-release` instead of a version, retiring any open proposal with `held: …`, through `next --published <tag[,tag]>` filled by `auto-release.yml` from `gh release list --exclude-drafts` (absent means "not told", the backwards-compatible default; present-but-empty means "nothing has published" and blocks every bump); `release.yml` serializes runs on the effective release tag with `queue: max`, and `scripts/publish-npm.sh` sends an out-of-order publish out under `--tag backfill` so npm's `latest` never moves backwards. Release mechanics and the bump policy table live in `docs/releasing.md` ("Cutting a release").
- **Artifact Provenance Contract**: language-neutral verification of release-artifact provenance (e.g., `artifact-provenance` sensor). Required inputs: artifact path(s) (`ARTIFACT_PATHS`), expected digest (`EXPECTED_DIGEST`), and verification mode (`VERIFY_MODE`). Evidence output must emit a `COVERAGE: <json>` record containing `artifact`, `digest`, `status` (`pass`|`fail`), and `reason` on failure. Only `digest-only` is implemented: reserved modes (`slsa`, `github-attestation`, `sbom`, `strict`) fail closed rather than reporting a pass they did not verify.
- **Root discovery**: the CLI walks up from cwd for do-harness.toml, or AGENTS.md with .do-harness/ or plans/invariants.json; override with --root. An explicit `--root DIR` selects exactly `<resolved DIR>/do-harness.toml` as a required file: policy-consuming commands (`verify`, `list`, `explain`, `status`, hook install) exit 2 before sensors, beat recording, or evidence writes when it is missing, unless `--config FILE` names another policy file. No ancestor or built-in-pack fallback applies to an explicitly targeted directory.

---

## 5. Concrete Rust Conventions
- **Error Handling**: `thiserror` enums in libraries, `anyhow` in binaries. No `unwrap()` in library code.
- **Safety**: `#![forbid(unsafe_code)]` at workspace and crate roots.
- **Config & Events**: `#[serde(deny_unknown_fields)]` on config and event structs.
- **Constants**: Named constants over magic numbers.
- **Async**: `#[tokio::main(flavor = "current_thread")]` for CLI binaries; sync `main` when no async is required. Follow workload-aware Tokio guidance (template #336, `.agents/skills/tokio-performance/SKILL.md`): short bounded CPU (<10–100µs) stays on worker threads; reserve `spawn_blocking`/pools for genuinely blocking disk/FFI or sustained heavy CPU (>1ms). Review flags: unbounded spawn fan-out, lock-across-`.await`, and repeatedly-ready loops.
- **Commits**: Conventional commits with lowercase subject lines.

## Commit convention

This repository enforces its commit convention with commitlint.

- Read the rules before committing: `npx commitlint --print-config json`
- Validate a message before using it: `printf '%s' "<message>" | npx commitlint` (exit 0 = valid)
- If the `commit-msg` hook rejects a commit, fix the rules named in brackets (e.g. `[subject-case]`) and retry.
- **Never use `git commit --no-verify`.**
- **Rule: The entire subject line (type, scope, and description) must be lowercase.** For example: `feat(ci): add guardrails` — not `feat(ci): add CI guardrails`.
- **Dependencies**: Declare versions in `[workspace.dependencies]` with caret ranges; commit `Cargo.lock` for binary applications.

---

## 6. Harness Engineering Model (Feedforward + Feedback)
- **Feedforward guides** (read before coding): `AGENTS.md`, `.agents/skills/` — context, constraints, conventions.
- **Feedback sensors** (fire after coding): `cargo check`, `cargo test`, `cargo clippy`, `cargo fmt --check` — computational output strictly supersedes LLM self-assessment.
- **Self-correction protocol**: classify the failure, apply the minimal fix, re-run the specific sensor, proceed only when green. See `.agents/skills/harness`.
- **Steering loop**: if a sensor fires more than 2 times in one sprint, update the matching feedforward guide (or create one via `.agents/skills/skill-distiller`) so sensors fire less over time.
- **Source grounding**: claims about tool behaviour or "best practice" cite the current official document, fetched and quoted at the time (upstream man/adoc page, vendor docs, spec) with the version verified against; search-result snippets are pointers, never evidence.
- **Metrics events**: after non-trivial tasks, write a JSON event to `.agents/events/YYYY/MM/DD/`.

---

## 7. Executable Architecture Invariants (Replacing Free-Form ADRs)

Architecture rules are machine-verifiable, not prose. Three structures:

1. **Schema Contracts** (`crates/types`): Rust types encode the rules — `DecisionHeader { invariant, rationale, sensor, category }`, typed HTN (`Method`, `Subtask`, `Precondition`, `TaskState`), and event-sourcing traits (`Command`, `DomainEvent`, `Projection`). Invalid states are unrepresentable; `#[serde(deny_unknown_fields)]` rejects stale payloads.
2. **Static Invariants** (linters & scripts): enforced computationally —
   - 500 LOC ceiling: `scripts/check-loc.sh`
   - dependency direction (`types` must not depend on storage/adapters): `scripts/check-deps.sh` + `cargo deny check` (`deny.toml`)
   - `cargo clippy -- -D warnings` (`workspace.lints`, `.clippy.toml`)
3. **Machine-Readable Decision Headers**: each decision carries `Invariant` / `Rationale` / `Sensor`. Source of truth: `plans/invariants.json`; persisted to libSQL via `cargo run -p do-harness-db --bin seed_invariants` (upsert into the `invariants` table).
