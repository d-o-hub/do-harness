# do-harness

A compiled agent-execution harness CLI: feedforward guides (AGENTS.md, `.agents/skills/`) plus feedback sensors (computational checks) that agents and CI must pass.

## Install

Zero-install with npx on Linux and macOS (Node 18+; resolves the prebuilt
platform binary). **Windows is not available through npm** — use the release
zip below:

```bash
npx do-harness init
npx do-harness verify --set verification --changed --strict
```

Prebuilt binaries for Linux (x86_64/aarch64, static musl), macOS
(x86_64/arm64), and Windows (x86_64, zip; included starting with `v0.1.1`;
`v0.1.0` shipped Linux/macOS only) are published as GitHub Releases
with SHA-256 checksums. The Windows zip is the only supported Windows
install path, because npm rejects the `do-harness-win32-x64` package name
(HTTP 403, "Package name triggered spam detection"); see
[docs/adoption.md](docs/adoption.md#windows) for details.

```bash
# latest release (resolved from the releases/latest redirect)
curl -fsSL https://raw.githubusercontent.com/d-o-hub/do-harness/main/scripts/install.sh | sh

# pinned, reproducible install
curl -fsSL https://raw.githubusercontent.com/d-o-hub/do-harness/main/scripts/install.sh \
  | sh -s -- --version v0.1.1
```

The installer verifies the artifact against the release `checksums.txt` before
installing to `$HOME/.local/bin` (override with `--bin-dir` /
`DO_HARNESS_INSTALL_DIR`). Checksums share the release origin, so they detect
corruption and truncated downloads, not a compromised origin. See
[docs/provenance-trust-model.md](docs/provenance-trust-model.md) for the complete
provenance trust boundary and verification model.

Rust users can also install from crates.io (source build) or fetch the
prebuilt release through `cargo-binstall`:

```bash
cargo install do-harness --version 0.1.1
# or, using the published release assets:
cargo binstall do-harness
```

Building from source requires Rust 1.85+:

```bash
cargo build --release -p do-harness
```

The binary lands at `target/release/do-harness`.

To install a source checkout on `PATH` (for use in other repositories):

```bash
cargo install --path crates/do-harness
```

## Use in another repo

The harness is designed to be adopted by any codebase, Rust or not:

1. Install the CLI onto `PATH` as the primary adoption flow — the prebuilt
   installer for any stack, or `cargo install --path` for a vendored or
   air-gapped checkout:

   ```bash
   curl -fsSL https://raw.githubusercontent.com/d-o-hub/do-harness/main/scripts/install.sh \
     | sh -s -- --version v0.1.1
   # or, from a vendored checkout:
   cargo install --path vendor/do-harness/crates/do-harness
   ```

   > **Warning**: Do NOT build `do-harness` into the adopting repository's shared `target/` directory (e.g. `cargo build --manifest-path vendor/do-harness/Cargo.toml -p do-harness --target-dir target`). Running `cargo clean` in the adopting workspace will delete the binary and silently break installed git hooks!

   Then from the target repository root run:

   ```bash
   do-harness init                 # Rust sensor pack
   do-harness init --language generic   # no built-in sensors
   do-harness init --language web       # web UI audit pack (viewport, a11y, console, perf, visual, i18n)
   do-harness init --language node      # Node/JS/TS pack (typecheck, lint, test, build)
   ```

   This writes `do-harness.toml`, `AGENTS.md`, `plans/invariants.json`,
   `.agents/skills/` (the `harness` skill plus `skill-creator` for building
   project-specific skills), and `.gitignore` entries, then initializes the
   local libSQL state and seeds the invariants. Existing files are left
   untouched unless you pass `--force`. When no `Cargo.toml` exists, the rust
   pack also scaffolds a minimal crate (`Cargo.toml` + `src/lib.rs`) so
   `init && verify` is green on a truly empty tree; existing crates are never
   touched, not even with `--force`. `init` then runs the generated contract
   once and reports `Initial verification: GREEN | RED | VACUOUS`, exiting
   non-zero on RED instead of claiming a verified workspace.

   The generated `AGENTS.md` is a routing/completion contract only. This
   repository's own `AGENTS.md` and `.agents/skills/` describe how to develop
   do-harness itself (HTN planning, spikes, event modeling, ATDD, skill
   distillation); those development-methodology skills are intentionally not
   scaffolded into adopting projects.

2. Configure sensors for your stack in `do-harness.toml`. Language packs:
   `rust` (fmt/check/clippy/test/loc/deps/audit/commitlint), `generic`
   (ships no sensors — add your own `[[sensors]]` entries), `web` (web UI
   audit pack: viewport text, axe accessibility, console noise, performance, visual
   diffs, i18n), and `node` (Node/JS/TS pack: typecheck, lint, test, build).
   With zero sensors `verify` exits 0 without running any command: a vacuous
   pass, not evidence. Define real sensors before treating verify output as proof.

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

# Prebuilt binary (pinned):
curl -fsSL https://raw.githubusercontent.com/d-o-hub/do-harness/main/scripts/install.sh \
  | sh -s -- --version v0.1.1
# ...or a vendored checkout (installs outside the workspace target/):
# cargo install --path vendor/do-harness/crates/do-harness

# Initialize and verify harness setup
do-harness hook install
do-harness doctor
do-harness verify
```

See [docs/adoption.md](docs/adoption.md) for per-ecosystem quickstarts,
air-gapped mirrors, and the agent evidence loop. See [docs/fast-builds.md](docs/fast-builds.md)
for opt-in `sccache` + `mold` build performance notes and measurements.

The green path is dogfooded, not assumed: `crates/do-harness/tests/dogfood.rs`
runs the real binary on fresh temp workspaces and asserts the rust pack goes
green after `init`, goes red once the crate is removed, and that the generic
pack's pass is vacuous. CI repeats `init && verify` on every push (see
`.github/workflows/verify.yml`).

For CI, install the pinned binary first, then invoke `do-harness verify --format json --evidence .do-harness/evidence.json --strict` (exit 0/1/2) — no build step required.

```bash
curl -fsSL https://raw.githubusercontent.com/d-o-hub/do-harness/main/scripts/install.sh \
  | sh -s -- --version v0.1.1
export PATH="$HOME/.local/bin:$PATH"
```

## Commands

| Command | Description |
|---------|-------------|
| `verify` | Run all sensors (flags: `--fail-fast`, `--format text\|json`, `--set SET`, `--changed`, `--only NAME` repeatable; `--record` persists beats + error signatures; `--evidence PATH` writes evidence artifact; `--strict` fails on weak evidence; `--bless` lowers/initializes `plans/baselines.json` findings ratchets, with `--approver NAME`) |
| `list` | Print sensor names (`--format text\|json`, `--sets` for signal-set names) |
| `explain` | Explain which sensors the current change selects, without running them (`--set SET`, `--changed`) |
| `status` | Report evidence freshness (`green\|red\|stale\|missing`) for `--set SET` without running sensors |
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
| `eval [--skill NAME] [--bless] [--agent-cmd CMD]` | Validate skills via skill-creator's quick_validate.py and persist skill_evals; `--bless` re-baselines graders and raises the pass-rate bar after a fully green run; `--agent-cmd` runs an external agent per case for true Skill Lift |
| `metrics [--format text\|json]` | Report sensor stats, strike counts, and eval pass-rate history |
| `dora [--days N] [--format text\|json] [--record] [--source git\|gh] [--now UNIX]` | Derive DORA metrics (deployment frequency, lead time, change failure rate, time to restore) from git history plus the pinned `plans/dora.json` policy; every number carries its derivation manifest |
| `compliance [--format text\|json]` | Print compliance mapping to OWASP Agentic Top 10, NIST AI RMF, and EU AI Act |
| `init [--language rust\|generic\|web\|node] [--force]` | Scaffold a harness workspace in the current directory |
| `hook install [--force]` / `hook uninstall` / `hook status` / `hook diff` | Manage git hooks (`.git/hooks/pre-commit`, `commit-msg`, `pre-push`) |
| `doctor` | Run diagnostic checks on binary resolution, git hook health, and state-database migration skew (fails when the database outruns the binary) |
| `pr no-effect <PR\|--base REV --head REV>` | Report whether a PR or revision range introduces any effective change, from the merge-base tree delta (read-only; works in any git repo) |
| `pr review <PR\|--base REV --head REV> [--recompute]` | Emit the semantic residual evidence could not prove, with skipped units and revocable `false_proven` claims (read-only) |
| `skills suggest --query TEXT [--limit N] [--format text\|json]` | Rank skills by frontmatter metadata for progressive disclosure (offline; metadata only, never a skill body); `DO_HARNESS_SKILL_SELECTOR` optionally reorders the shortlist |
| `skills drift [--manifest FILE] [--format text\|json]` | Check manifest-managed shared skills against their pinned commit and tree digest (offline, check-only; `2` when the manifest is absent or invalid, or a managed tree cannot be described) |
| `version [--format text\|json]` | Print the running binary's name, version, commit, commit date, and dirty flag |
| `loc [PATH]... [--warn] [--format text\|json]` | Report each file's line count against the 500-line ceiling with its code/test split (`--warn` shows only files at or above the 450-line threshold) |
| `split <FILE> [--target NAME] [--dry-run]` | Extract a large top-level item or inline test module into a sibling module, refusing shapes it cannot make compile |
| `overlap [--threshold N] [--format text\|json]` | Rank skill pairs by guidance overlap; a pair at or above the threshold (default 0.45) prints as WARN (Tier-2 distinctiveness advisory) |
| `maintenance [--prune-beats DAYS] [--keep-per-task N]` | Prune old beats and compact the local state database |
| `audit-chain [--format text\|json]` | Recompute the workflow event hash chain and report the first divergence |
| `completions <bash\|zsh\|fish\|powershell\|elvish>` | Generate shell completions |
| `man <DIR>` | Generate man pages into a directory |

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

`do-harness hook install` writes three managed hooks into `.git/hooks/`:

- **pre-commit**: runs `verify --fail-fast --only fmt --only loc`
- **commit-msg**: runs `scripts/check-commitlint.sh --message` against the prepared message (fail-closed: blocks the commit when the repo or script is missing)
- **pre-push**: runs the full `verify` suite

Managed hooks carry a marker identifying them as do-harness-owned; re-running `hook install` overwrites them, and `--force` overwrites any pre-existing hook at the same path. `hook uninstall` removes only managed hooks. `hook status` reports which hooks are present and whether they are managed.

Alternative for users of the pre-commit framework: this repo ships `.pre-commit-config.yaml` with a `local` hook (`language: system`) whose entry is `./target/release/do-harness verify --fail-fast --only fmt --only loc`.

## CI

GitHub Actions: see `.github/workflows/verify.yml` — lints shell sensors, builds `do-harness`, checks the adjacent `guardian-proxy` crate with the optional features off and on (`cargo check -p guardian-proxy` / `--features agt-governance` / `--features mcp-surface`) and tests it (`cargo test -p guardian-proxy` plus both feature builds), runs `verify --set verification --format json --strict` and `status --set verification`, dogfoods `init && verify` on fresh rust/generic workspaces, and runs `do-harness eval`.

> **Runtime proxy note:** `do-harness` itself stays a dev-loop harness (no traffic proxy). The adjacent `crates/guardian-proxy` is an *optional* fail-closed sidecar (off by default; governance via `agt-governance`, MCP Streamable HTTP ingress via `mcp-surface`) and the only tool-call mediation surface, via its `ProxyMediator` gate — see `crates/guardian-proxy/README.md` when present.

## DeepSeek Harness integration

`integrations/deepseek-harness/` ships an out-of-tree DSH bundle that exposes
do-harness development signals as one typed `development_signals` tool and an
optional strict completion gate. The Rust core has no DeepSeek, Cordis, or
Node.js dependency, and removing the bundle leaves normal CLI use unaffected.
All verification policy stays in the CLI; the bundle only invokes it through
DSH's managed subprocess seam. See
[`integrations/deepseek-harness/README.md`](integrations/deepseek-harness/README.md)
for the install walkthrough and configuration.

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

See [docs/compliance.md](docs/compliance.md) for full mappings against the **OWASP Agentic Top 10 (2026)**, **NIST AI RMF 1.0**, and the **EU AI Act**.

## Configuration

`do-harness.toml` at the workspace root configures the harness:

- `language` — the language pack (`"rust"`, `"generic"`, `"web"`, or `"node"`;
  the generic pack ships no sensors).
- `[hooks]` — `pre-commit` / `pre-push` lists naming the sensors each hook runs; an empty `pre-push` list means the full suite.
- `[[sensors]]` — each sensor has a `name` and an `argv` (the command to execute).
  Optional `when-changed` globs declare when the sensor applies to the current
  change (`verify --changed` / `explain`); sensors without it always run.
- `[signal-sets]` — named sensor selections for a decision (`feedback` for the
  edit loop, `verification` for pre-completion, `release` for releases).
  `verify --set <name>` runs only that set; without `[signal-sets]`,
  `verification`/`release` mean the full list and `feedback` is rejected.

When no config is found, the CLI falls back to the built-in Rust sensor pack. The config file is also the workspace-root marker used for discovery.

## License

MIT. See `LICENSE`.
