# AGENTS.md — Autonomous & Interactive Agent Execution Harness

## 1. System Operating Invariants (Hard Constraints)
- **Modularity Cap**: Maximum 500 Lines of Code (LOC) per source file. Decompose when nearing 450 LOC.
- **Persistence Engine**: Local libSQL instance (`.do-harness/agent_state.db`) for tracking beats, task execution traces, and learned heuristics.
- **Persistence & Roadmap**: the do-harness-db repo layer implements persistence writers for tasks/beats/traces/heuristics/error_signatures/skill_evals (`verify --record` persists beats scoped to `--task` with the sensor name, resetting a sensor's strikes on pass; `task list`/`task export` read task state). The workflow runtime (`task add/advance/done/fail`, gated on the `plans/methods.json` catalog and the **named** sensor's ok beat, with typed Commands/Events and a `TaskBoard` projection), fail-fast recovery (`errors list/clear`, per-task strike counters), the skill-eval runner (`do-harness eval` grades hermetic walkthrough residue in an isolated sandbox, gated by skill-creator's quick_validate.py, and persists the latest pass rate per skill), the distill CLI (`distill` writes into the skill corpus + `trace add/list`), and commit enforcement (`check-commitlint.sh` as both a `commitlint` sensor and a managed `commit-msg` git hook) are all implemented. Known remaining hardening is tracked as follow-ups, not hidden behind a roadmap claim.
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
- Run the full verification suite via the unified entrypoint (`do-harness verify` runs all six computational sensors):
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
- **Hooks**: `do-harness hook install` writes .git/hooks/pre-commit (fmt + loc) and pre-push (full verify); uninstall/status remove/inspect managed hooks.
- **Workflow gates**: pre-commit = `do-harness verify --fail-fast --only fmt --only loc`; pre-push = `do-harness verify --fail-fast`; CI = `do-harness verify --format json` (exit 0/1/2).
- **Persistence commands**: `do-harness task list`/`task export` surface task state (libSQL is the source of truth; export writes `plans/tasks.json`); `verify --record` persists beats and bumps error signatures for failing sensors; `task add/advance/fail`, `trace add/list`, `distill`, `eval` are implemented.
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
