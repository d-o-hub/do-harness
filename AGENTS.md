# AGENTS.md — Autonomous & Interactive Agent Execution Harness

## 1. System Operating Invariants (Hard Constraints)
- **Modularity Cap**: Maximum 500 Lines of Code (LOC) per source file. Decompose when nearing 450 LOC.
- **Persistence Engine**: Local libSQL instance (`.do-harness/agent_state.db`) for tracking beats, task execution traces, and learned heuristics.
- **Persistence & Roadmap**: the do-harness-db repo layer implements persistence writers for tasks/beats/traces/heuristics/error_signatures/skill_evals (`verify --record` persists beats scoped to `--task` with the sensor name, resetting a sensor's strikes on pass; `task list`/`task export` read task state). The workflow runtime (`task add/advance/done/fail`, gated on the `plans/methods.json` catalog and the **named** sensor's ok beat, with typed Commands/Events and a `TaskBoard` projection), fail-fast recovery (`errors list/clear`, per-task strike counters), the skill-eval runner (`do-harness eval` grades hermetic walkthrough residue in an isolated sandbox — walkthroughs execute directly so their shebang picks the interpreter, and failures surface their stderr tail; gated by skill-creator's quick_validate.py; persists the latest pass rate per skill **plus** append-only `skill_eval_runs` history, with Skill Lift (with-skill minus without-skill baseline) ratcheted by blessed floors, case kinds tagged explicit/implicit/contextual/negative with `eval --strict-fixtures` (CI) rejecting thin datasets, and `eval --agent-cmd` running an external agent per case for real agent lift with the mode persisted per run), grader tamper-evidence (`eval --bless` pins SHA-256 baselines of `walkthrough.sh`/`evals.json`; drift fails eval until re-blessed) with a pass-rate bar ratchet (floor = best-ever − tolerance, blessed-only raises; `distill --to-fixture` raises it after a recovered failure), the `metrics` command (sensor stats, strikes, skill trends), the distill CLI (`distill` writes into the skill corpus + `trace add/list`), and commit enforcement (`check-commitlint.sh` as both a `commitlint` sensor and a managed `commit-msg` git hook) are all implemented. The runtime-neutral development-signal contract (`[signal-sets]` + `verify --set`, `when-changed` + `verify --changed`/`explain`, workspace/policy fingerprints + evidence schema v3 + `status` green|red|stale|missing, and evidence-driven `init` baseline proof) is implemented with its epic in `plans/development-signals-epic.md`; the out-of-tree DeepSeek Harness bundle lives in `integrations/deepseek-harness/` and holds no verification policy. Remaining hardening is the GA-gated `chore-agt-promotion` decision (`plans/agt-governance-epic.md` Next action); issues #34 (refactor), #35 (Rust hygiene), #37 (db), #38 (CI/sensors/evidence), and #40 (proxy/eval security) were merged to `main` via PR #50 (squash `cd5b291`) and closed by its `Closes` trailers. Sensor policy decisions live in `plans/invariants.json`. The `shell` sensor lints every repository and skill shell script (including `evals/walkthrough.sh`), and `check-agt` runs the feature-on guardian-proxy tests; sensors whose tool is unavailable print `SKIP:`, which `verify` reports as WARN and evidence records as a non-pass (local gate stays usable while `--strict`/`status` stay honest). Concurrent-writer recovery, evidence-struct ownership, truth-decay sweep, skipped-tool evidence, template-reference sync, and shallow-clone branch triage heuristics were distilled into `.agents/skills/harness/references/heuristics.md` (traces 14–23). MCP conformance for `guardian-proxy` is implemented behind the optional `mcp-surface` feature (spike decision: adopt `rmcp`, ingress only; hand-rolled and re-scope rejected) with the plan in `plans/mcp-conformance-epic.md` (trace 24); the flat tool-call route is deprecated, and `chore-mcp-promotion` (2026-09-13) kept the surface off by default with MSRV 1.85 because route removal is coupled to a default-enabled successor. Agent integration deliberately stays skills + JSON contracts: no do-harness MCP wrapper ships (invariant in `plans/invariants.json`, sensor `do-harness eval`), because MCP cannot enforce the completion gate and host-side sensor execution would change the trust model. Release distribution is implemented (`plans/distribution-epic.md`, tasks 40–43): the tag-triggered `.github/workflows/release.yml` publishes Linux static-musl and macOS x86_64/aarch64 binaries with `checksums.txt` (`v0.1.0` published 2026-09-13), and `scripts/install.sh` installs a pinned release after SHA-256 verification; `init` embeds the pinned installer command in the generated `AGENTS.md`. crates.io publishing (`do-harness-types` → `do-harness-db` → `do-harness`, idempotent and tag-gated) plus cargo-binstall metadata (QuickInstall disabled) followed in tasks 44–46; the CLI embeds repository assets through symlinks that `cargo package` dereferences, and the greenfield crate scaffold lives at `templates/crate/Cargo.toml.template` because cargo excludes nested packages. A zero-install npm wrapper (`integrations/npm/`, tasks 47–49) publishes four platform packages plus a `do-harness` meta package from the tag-gated `npm-publish` job, so `npx do-harness ...` runs without installing the CLI; the shim resolves the platform package through `optionalDependencies` and holds no verification policy. Windows x86_64 support (tasks 51–53) adds an msvc zip release artifact, a `win32-x64` npm platform package, `.exe` hook resolution, and a `windows-latest` CI job that dogfoods `init && verify` under Git Bash.
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
  ```bash
  do-harness verify
  ```
- For targeted re-runs after a specific sensor fires (self-correction protocol), invoke the individual command:
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
- **Hooks**: `do-harness hook install` writes .git/hooks/pre-commit (fmt + loc, run with `--record` so beats persist), commit-msg (commitlint), and pre-push (full verify); uninstall/status remove/inspect managed hooks. Hooks resolve the binary from `DO_HARNESS_BIN`, PATH, or `target/release/do-harness` (build it or set the env var in dev).
- **Workflow gates**: pre-commit = `do-harness verify --fail-fast --only fmt --only loc`; pre-push = `do-harness verify --fail-fast`; CI = `verify.yml` lints shell + `cargo check -p guardian-proxy --features agt-governance` + `do-harness verify --set verification --format json --strict` (exit 0/1/2) + `do-harness status --set verification` (must be green) + dogfood `init && verify` + `do-harness eval`.
- **Persistence commands**: `do-harness task list`/`task export` surface task state (libSQL is the source of truth; export writes `plans/tasks.json` with a board summary and `task import --check` validates freshness); `verify --record` persists beats and bumps error signatures for failing sensors; `task add/advance/fail`, `trace add/list`, `distill`, `eval`, `metrics`, and `maintenance --prune-beats` are implemented.
- **Signal commands**: `verify --set <set>` runs one development decision's sensors (writing `.do-harness/evidence.<set>.json`); `verify --changed` / `explain --set <set> --changed` apply deterministic `when-changed` selection (fail-closed when git state is unreadable); `status --set <set>` reports evidence freshness `green|red|stale|missing` without running sensors; `list --sets` enumerates configured sets.
- **Sensor policy decisions**: explicit wontfix entries (gitleaks, SBOM, lockfile freshness, eval bless-drift) live in `plans/invariants.json`; `do-harness seed --prune` keeps libSQL in sync and drops stale rows. `shell` (shellcheck, fail-closed in CI) and `check-agt` (guardian-proxy governance feature) are workspace sensors.
- **Root discovery**: the CLI walks up from cwd for do-harness.toml, or AGENTS.md with .do-harness/ or plans/invariants.json; override with --root.

---

## 5. Concrete Rust Conventions
- **Error Handling**: `thiserror` enums in libraries, `anyhow` in binaries. No `unwrap()` in library code.
- **Safety**: `#![forbid(unsafe_code)]` at workspace and crate roots.
- **Config & Events**: `#[serde(deny_unknown_fields)]` on config and event structs.
- **Constants**: Named constants over magic numbers.
- **Async**: `#[tokio::main(flavor = "current_thread")]` for CLI binaries; sync `main` when no async is required.
- **Commits**: Conventional commits with lowercase subject lines.
- **Dependencies**: Declare versions in `[workspace.dependencies]` with caret ranges; commit `Cargo.lock` for binary applications.

---

## 6. Harness Engineering Model (Feedforward + Feedback)
- **Feedforward guides** (read before coding): `AGENTS.md`, `.agents/skills/` — context, constraints, conventions.
- **Feedback sensors** (fire after coding): `cargo check`, `cargo test`, `cargo clippy`, `cargo fmt --check` — computational output strictly supersedes LLM self-assessment.
- **Self-correction protocol**: classify the failure, apply the minimal fix, re-run the specific sensor, proceed only when green. See `.agents/skills/harness`.
- **Steering loop**: if a sensor fires more than 2 times in one sprint, update the matching feedforward guide (or create one via `.agents/skills/skill-distiller`) so sensors fire less over time.
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
