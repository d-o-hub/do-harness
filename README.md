# do-harness

A compiled agent-execution harness CLI: feedforward guides (AGENTS.md, `.agents/skills/`) plus feedback sensors (computational checks) that agents and CI must pass.

## Install

Requirements: Rust 1.85+.

```bash
cargo build --release -p do-harness
```

The binary lands at `target/release/do-harness`.

To install it on `PATH` (for use in other repositories):

```bash
cargo install --path crates/do-harness
```

## Use in another repo

The harness is designed to be adopted by any codebase, Rust or not:

1. Install the CLI onto `PATH` as the primary adoption flow:

   ```bash
   # From do-harness source or vendored path:
   cargo install --path crates/do-harness
   # or when vendored inside an adopting repository:
   cargo install --path vendor/do-harness/crates/do-harness
   ```

   > **Warning**: Do NOT build `do-harness` into the adopting repository's shared `target/` directory (e.g. `cargo build --manifest-path vendor/do-harness/Cargo.toml -p do-harness --target-dir target`). Running `cargo clean` in the adopting workspace will delete the binary and silently break installed git hooks!

   Then from the target repository root run:

   ```bash
   do-harness init                 # Rust sensor pack
   do-harness init --language generic   # no built-in sensors
   ```

   This writes `do-harness.toml`, `AGENTS.md`, `plans/invariants.json`,
   `.agents/skills/`, and `.gitignore` entries, then initializes the local
   libSQL state and seeds the invariants. Existing files are left untouched
   unless you pass `--force`. When no `Cargo.toml` exists, the rust pack also
   scaffolds a minimal crate (`Cargo.toml` + `src/lib.rs`) so `init && verify`
   is green on a truly empty tree; existing crates are never touched, not
   even with `--force`.

2. Configure sensors for your stack in `do-harness.toml`. Language packs:
   `rust` (fmt/check/clippy/test/loc/commitlint) and `generic` (ships no
   sensors — add your own `[[sensors]]` entries). With zero sensors
   `verify` exits 0 without running any command: a vacuous pass, not
   evidence. Define real sensors before treating verify output as proof.

3. Wire the git hooks:

   ```bash
   do-harness hook install
   ```

   Hooks locate the binary at runtime: `$DO_HARNESS_BIN`, then `do-harness`
   on `PATH`, then `<repo>/target/release/do-harness`.

4. Run diagnostics and the suite:

   ```bash
   do-harness doctor               # verify binary resolution, git hooks & state-db migration skew
   do-harness verify               # run sensor suite
   ```

### Dev setup & CI snippet

For local developer setup scripts or CI workflows in adopting repositories:

```bash
#!/usr/bin/env bash
set -euo pipefail

# Install CLI to Cargo bin path (outside workspace target/)
cargo install --path vendor/do-harness/crates/do-harness

# Initialize and verify harness setup
do-harness hook install
do-harness doctor
do-harness verify
```

The green path is dogfooded, not assumed: `crates/do-harness/tests/dogfood.rs`
runs the real binary on fresh temp workspaces and asserts the rust pack goes
green after `init`, goes red once the crate is removed, and that the generic
pack's pass is vacuous. CI repeats `init && verify` on every push (see
`.github/workflows/verify.yml`).

For CI, invoke `do-harness verify --format json` (exit 0/1/2) with the CLI on
`PATH` — no build step required.

## Commands

| Command | Description |
|---------|-------------|
| `verify` | Run all sensors (flags: `--fail-fast`, `--format text\|json`, `--only NAME` repeatable; `--record` persists beats + error signatures) |
| `list` | Print sensor names (`--format text\|json`) |
| `init-db` | Apply migrations to `.do-harness/agent_state.db` |
| `seed` | Upsert `plans/invariants.json` into the DB |
| `task list [--format text\|json]` / `task export` | Read task state from the local database; export writes `plans/tasks.json` |
| `task add <title> [--method NAME] [--parent ID] [--precondition TEXT]` | Add a pending task to the workflow runtime |
| `task advance <ID>` | Advance the subtask pointer |
| `task done <ID>` | Mark a task done once all sensor-gated subtasks have passed |
| `task fail <ID>` | Mark a task failed |
| `errors list [--task ID] [--format text\|json]` / `errors clear [--sensor NAME] [--task ID]` | Inspect and clear fail-fast error signatures |
| `trace add --session S [--task ID] [--command C] [--error-diff D] [--resolution-steps R]` / `trace list --session S [--format text\|json]` | Record and read execution traces per session |
| `distill --skill NAME --pattern P [--description D] [--from-trace ID] [--to-fixture]` | Distill a resolved trace into a skill (refuses without resolution steps); `--to-fixture` raises the skill's pass-rate bar |
| `eval [--skill NAME] [--bless]` | Validate skills via skill-creator's quick_validate.py and persist skill_evals; `--bless` re-baselines graders and raises the pass-rate bar after a fully green run |
| `metrics [--format text\|json]` | Report sensor stats, strike counts, and eval pass-rate history |
| `compliance [--framework NAME] [--format text\|json]` | Print compliance crosswalk for OWASP Agentic Top 10, NIST AI RMF, EU AI Act, and SOC 2 |
| `init [--language rust\|generic] [--force]` | Scaffold a harness workspace in the current directory |
| `hook install [--force]` / `hook uninstall` / `hook status` | Manage `.git/hooks/pre-commit` + `pre-push` |
| `doctor [--strict]` | Run diagnostic checks on binary resolution, git hook health, and state-database migration skew (use `--strict` to convert warnings to non-zero failures) |

Global flags: `--root <path>`, `--config <path>`.

## Exit codes

| Code | Meaning |
|------|---------|
| 0 | All sensors passed |
| 1 | A sensor failed |
| 2 | Usage/config/discovery error (e.g. not inside a do-harness workspace, missing git repo for `hook install`) |

## JSON output

`verify --format json` emits a single JSON object:

```json
{
  "ok": false,
  "root": "/path/to/workspace",
  "failed": ["clippy"],
  "sensors": [
    {"name": "fmt", "ok": true, "exit_code": 0, "duration_ms": 320},
    {"name": "clippy", "ok": false, "exit_code": 1, "duration_ms": 4100}
  ]
}
```

Failure output tails are printed to stderr so stdout stays parseable.

## Persistence

Task lifecycle commands (`task add`/`advance`/`done`/`fail`) append each
emitted workflow event to an append-only `workflow_events` log in the same
transaction as the task-row mutation; `task list` folds that real event
stream into its board summary.

`verify --record` persists each sensor result as a beat into
`.do-harness/agent_state.db` and bumps an error signature
(`sensor:<name>`) for every failing sensor. Once a sensor's error
signature reaches 3 consecutive failures, it is halted: it is not
executed and is reported failed with a "halted" diagnostic until the
underlying issue is resolved (strikes do not grow while halted). `task list` reads task state
from the same store (`--format text|json`); `task export` writes
`plans/tasks.json` as an agent-readable snapshot — libSQL remains the
source of truth.

## Hooks

`do-harness hook install` writes both hooks into `.git/hooks/`:

- **pre-commit**: runs `verify --fail-fast --only fmt --only loc`
- **pre-push**: runs the full `verify` suite

Managed hooks carry a marker identifying them as do-harness-owned; re-running `hook install` overwrites them, and `--force` overwrites any pre-existing hook at the same path. `hook uninstall` removes only managed hooks. `hook status` reports which hooks are present and whether they are managed.

Alternative for users of the pre-commit framework: this repo ships `.pre-commit-config.yaml` with a `local` hook (`language: system`) whose entry is `./target/release/do-harness verify --fail-fast --only fmt --only loc`.

## CI

GitHub Actions: see `.github/workflows/verify.yml` (builds `do-harness`, then runs `verify --format json`).

GitLab CI:

```yaml
verify:
  image: rust:latest
  script:
    - cargo build --release -p do-harness
    - ./target/release/do-harness verify --format json
```

## Compliance positioning

`do-harness` enforces deterministic, computational controls over the agent development loop (build-time verification sensors, workflow gates, tamper-evident logs, and machine-readable evidence artifacts). It is designed to satisfy dev-loop assurance requirements in regulated environments without over-claiming runtime policy or proxy capabilities.

See [docs/compliance.md](docs/compliance.md) for full mappings against the **OWASP Agentic Top 10 (2026)**, **NIST AI RMF 1.0**, **EU AI Act**, and **SOC 2 (Trust Services Criteria)**.

## Configuration

`do-harness.toml` at the workspace root configures the harness:

- `language` — the language pack (`"rust"` or `"generic"`; the generic pack
  ships no sensors).
- `[hooks]` — `pre-commit` / `pre-push` lists naming the sensors each hook runs; an empty `pre-push` list means the full suite.
- `[[sensors]]` — each sensor has a `name` and an `argv` (the command to execute).

When no config is found, the CLI falls back to the built-in Rust sensor pack. The config file is also the workspace-root marker used for discovery.

## Fail-closed semantics

`do-harness` follows explicit fail-closed contracts across execution gates:

- **Preconditions**: when a precondition is unmet (config missing or invalid, DB unreadable, binary stale, git absent), `do-harness` exits non-zero rather than skipping the affected sensor or command.
- **Config validation**: referencing an unknown or typo'd sensor name in `do-harness.toml` (e.g. under `[hooks]`) fails at parse/load time with a configuration error.
- **Strict diagnostics (`doctor --strict`)**: under `--strict`, all advisory warnings (binary under Cargo `target/`, absent git hooks, pending/uninitialized state DB, unreadable/tampered event chain) are elevated to fatal failures (exiting non-zero).

## License

MIT. See `LICENSE`.
