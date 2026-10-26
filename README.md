# do-harness

A compiled agent-execution harness CLI: feedforward guides (AGENTS.md, `.agents/skills/`) plus feedback sensors (computational checks) that agents and CI must pass.

Optional [client boundary conformance](docs/client-conformance.md) distinguishes
installed guidance, sanitizer fixture capability, and instrumented client
observations. A passing fixture alone never proves client mediation.

## Install

Zero-install with npx on Linux and macOS (Node 18+; resolves the prebuilt
platform binary). **Windows is not available through npm** — use the release
zip below:

<!-- example: id=npx_quickstart classification=illustrative reason="npx runner invocation" -->
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
[docs/adoption.md](https://github.com/d-o-hub/do-harness/blob/main/docs/adoption.md#windows) for details.

<!-- example: id=install_script classification=executable fixture=installer_local_artifact -->
```bash
# latest release (resolved from the releases/latest redirect)
curl -fsSL https://raw.githubusercontent.com/d-o-hub/do-harness/main/scripts/install.sh | sh

# pinned, reproducible install
curl -fsSL https://raw.githubusercontent.com/d-o-hub/do-harness/main/scripts/install.sh \
  | sh -s -- --version v0.2.1
```

The installer verifies the artifact against the release `checksums.txt` before
installing to `$HOME/.local/bin` (override with `--bin-dir` /
`DO_HARNESS_INSTALL_DIR`). Checksums share the release origin, so they detect
corruption and truncated downloads, not a compromised origin. See
[docs/provenance-trust-model.md](https://github.com/d-o-hub/do-harness/blob/main/docs/provenance-trust-model.md) for the complete
provenance trust boundary and verification model.

Rust users can fetch the prebuilt release through `cargo-binstall`:

<!-- example: id=cargo_binstall classification=illustrative reason="cargo binstall tool" -->
```bash
cargo binstall do-harness
```

Registry source installs through 0.2.1 do not include the checkout's libSQL
teardown fix; `--locked` does not restore that missing code. That fix ships from
0.3.0 on — published with registry validation on musl and Windows — so source
installs from 0.3.0 use the maintained `do-harness-libsql` fork:
`cargo install do-harness --version 0.3.0 --locked`.
See [source-distribution validation](docs/releasing.md#source-distribution-validation).

Building from source requires Rust 1.85+:

<!-- example: id=cargo_build_release classification=illustrative reason="Cargo build command" -->
```bash
cargo build --release --locked -p do-harness
```

The binary lands at `target/release/do-harness`.

To install a source checkout on `PATH` (for use in other repositories):

<!-- example: id=cargo_install_crates classification=illustrative reason="Cargo install path command" -->
```bash
cargo install --path crates/do-harness --locked
```

## Use in another repo

The harness is designed to be adopted by any codebase, Rust or not:

1. Install the CLI onto `PATH` as the primary adoption flow — the prebuilt
   installer for any stack, or `cargo install --path` for a vendored or
   air-gapped checkout:

   <!-- example: id=use_another_repo_install classification=illustrative reason="Alternative adoption installation flows" -->
   ```bash
   curl -fsSL https://raw.githubusercontent.com/d-o-hub/do-harness/main/scripts/install.sh \
     | sh -s -- --version v0.2.1
   # or, from a vendored checkout:
   cargo install --path vendor/do-harness/crates/do-harness
   ```

   > **Warning**: Do NOT build `do-harness` into the adopting repository's shared `target/` directory (e.g. `cargo build --manifest-path vendor/do-harness/Cargo.toml -p do-harness --target-dir target`). Running `cargo clean` in the adopting workspace will delete the binary and silently break installed git hooks!

   Then from the target repository root run:

   <!-- example: id=init_language_packs classification=executable fixture=generic_adoption -->
   ```bash
   do-harness init                      # detect the pack from the repository
   do-harness init --language rust      # Rust sensor pack, explicit
   do-harness init --language generic   # no built-in sensors
   do-harness init --language web       # web UI audit pack (viewport, a11y, console, perf, visual, i18n)
   do-harness init --language node      # Node/JS/TS pack (typecheck, lint, test, build)
   ```

   Plain `init` detects the target instead of assuming Rust: an explicit
   `--language` wins, an existing `do-harness.toml` keeps its declared pack, a
   `Cargo.toml` selects rust, any other non-empty project selects the generic
   pack, and an empty directory bootstraps rust.

   The web pack's sensors require the `WEB_AUDIT_*` environment plus the
   `playwright`, `@axe-core/playwright`, and `lighthouse` peer dependencies
   before they run: an unconfigured web pack SKIPs, and `--strict` treats a
   SKIP as a failure. See [docs/adoption.md web pack
   configuration](https://github.com/d-o-hub/do-harness/blob/main/docs/adoption.md#web-pack-configuration).

   This writes `do-harness.toml`, `AGENTS.md`, `USE_RESTRICTIONS.md` (standalone
   policy for high-risk operations and human approval boundaries), `plans/invariants.json`,
   `.agents/skills/` (the `harness` skill plus `skill-creator` for building
   project-specific skills), and `.gitignore` entries, then initializes the
   local libSQL state and seeds the invariants. Existing files are left
   untouched unless you pass `--force`. When no `Cargo.toml` exists, the rust
   pack also scaffolds a minimal crate (`Cargo.toml` + `src/lib.rs`) so
   `init && verify` is green on a truly empty tree; existing crates are never
   touched, not even with `--force`. Every pack also gets an inert, commented
   recipe for the optional context-sanitization check
   (`scripts/check-context-sanitization.py`): the sensor is off unless you
   declare it and set `DO_HARNESS_CONTEXT_SANITIZATION=enabled`, and it never
   captures prompts, tool arguments, or vault contents. `init` then runs the
   generated contract once and reports `Initial verification: GREEN | RED | VACUOUS`, exiting
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
   For soft, project-specific facts checks (status-document drift, tracker
   counts) declare `kind = "project-check"`: warn-only unless `severity` says
   otherwise, with an optional `fix` hint printed on failure. `init` scaffolds
   the `scripts/check-tracker-drift.sh` reference implementation.

3. Wire the git hooks:

   <!-- example: id=readme_hook_install classification=illustrative reason="Hook install command snippet" -->
   ```bash
   do-harness hook install
   ```

   Hooks locate the binary at runtime: `$DO_HARNESS_BIN`, then `do-harness`
   on `PATH`, then `<repo>/target/release/do-harness`.

4. Run diagnostics and the suite:

   <!-- example: id=readme_doctor_verify classification=illustrative reason="Diagnostics and verify snippet" -->
   ```bash
   do-harness doctor               # verify binary resolution, git hooks & state-db migration skew
   do-harness verify               # run sensor suite
   ```

### Dev setup & CI snippet

For local developer setup scripts or CI workflows in adopting repositories:

<!-- example: id=dev_setup_script classification=illustrative reason="Multi-step developer setup script" -->
```bash
#!/usr/bin/env bash
set -euo pipefail

# Prebuilt binary (pinned):
curl -fsSL https://raw.githubusercontent.com/d-o-hub/do-harness/main/scripts/install.sh \
  | sh -s -- --version v0.2.1
# ...or a vendored checkout (installs outside the workspace target/):
# cargo install --path vendor/do-harness/crates/do-harness

# Initialize and verify harness setup
do-harness hook install
do-harness doctor
do-harness verify
```

See [docs/adoption.md](https://github.com/d-o-hub/do-harness/blob/main/docs/adoption.md) for per-ecosystem quickstarts,
air-gapped mirrors, and the agent evidence loop. See [docs/fast-builds.md](https://github.com/d-o-hub/do-harness/blob/main/docs/fast-builds.md)
for opt-in `sccache` + `mold` build performance notes and measurements.

The green path is dogfooded, not assumed: `crates/do-harness/tests/dogfood.rs`
runs the real binary on fresh temp workspaces and asserts the rust pack goes
green after `init`, goes red once the crate is removed, and that the generic
pack's pass is vacuous. CI repeats `init && verify` on every push (see
`.github/workflows/verify.yml`).

For CI, install the pinned binary first, then invoke `do-harness verify --format json --evidence .do-harness/evidence.json --strict` (exit 0/1/2) — no build step required.

<!-- example: id=ci_path_export classification=illustrative reason="CI environment path export snippet" -->
```bash
curl -fsSL https://raw.githubusercontent.com/d-o-hub/do-harness/main/scripts/install.sh \
  | sh -s -- --version v0.2.1
export PATH="$HOME/.local/bin:$PATH"
```

## Commands

<!-- The table below is generated from the CLI definitions; refresh it with
`python3 scripts/docs.py sync --write` (it refuses in CI). -->
<!-- BEGIN GENERATED: cli-commands -->
| Command | Description |
| --- | --- |
| `version [OPTIONS]` | Print CLI version information — flags: `--format <FORMAT>` |
| `verify [OPTIONS]` | Run computational sensors (alias: `check`) — flags: `--fail-fast`, `--format <FORMAT>`, `--set <SET>`, `--changed`, `--only <SENSOR>`, `--exclude <SENSOR>`, `--jobs <N>`, `--unchanged <MODE>`, `--record`, `--task <ID>`, `--global`, `--evidence <FILE>`, `--strict`, `--bless`, `--approver <NAME>` |
| `list [OPTIONS]` | List sensor names (alias: `ls`) — flags: `--sets`, `--format <FORMAT>` |
| `explain [OPTIONS]` | Explain which sensors the current change selects, without running them — flags: `--set <SET>`, `--changed`, `--format <FORMAT>` |
| `status [OPTIONS]` | Report verification evidence freshness without running sensors — flags: `--set <SET>`, `--evidence <FILE>`, `--format <FORMAT>` |
| `pr no-effect [OPTIONS] [PR]` | Report whether a PR or revision range introduces any effective change — flags: `--base <REV>`, `--head <REV>`, `--format <FORMAT>` |
| `pr review [OPTIONS] [PR]` | Emit the semantic residual: changed units evidence could not prove — flags: `--base <REV>`, `--head <REV>`, `--recompute`, `--format <FORMAT>` |
| `pr ready [OPTIONS] <PR>` | Check whether a pull request is ready to merge: merge state, CI checks, and review conversations (aliases: `readiness`, `check`) — flags: `--format <FORMAT>` |
| `pr waivers [OPTIONS] --lcov <PATH> [PR]` | Classify patch-coverage residue: waivable classes and lines needing tests — flags: `--base <REV>`, `--head <REV>`, `--patch <PATH>`, `--lcov <PATH>`, `--since <PATH>`, `--strip-prefix <PREFIX>`, `--format <FORMAT>` |
| `skills suggest [OPTIONS] --query <TEXT>` | Rank skills by relevance to a query using metadata only — flags: `--query <TEXT>`, `--limit <N>`, `--format <FORMAT>` |
| `skills drift [OPTIONS]` | Check manifest-managed shared skills against their pinned digests — flags: `--manifest <FILE>`, `--format <FORMAT>` |
| `skills refresh-validator [OPTIONS]` | Replace only the installed skill-creator structure validator |
| `init-db [OPTIONS]` | Apply pending database migrations — flags: `--check`, `-y, --yes` |
| `seed [OPTIONS]` | Seed invariants from plans/invariants.json — flags: `--prune` |
| `init [OPTIONS]` | Scaffold a harness workspace in a target directory — flags: `--language <LANG>`, `--force`, `--format <FORMAT>`, `--no-seed`, `--minimal`, `--no-gitignore` |
| `task export [OPTIONS]` | Write the task list to plans/tasks.json or specified output — flags: `--stdout`, `--format <FORMAT>` |
| `task import [OPTIONS]` | Validate plans/tasks.json against the state database — flags: `--file <FILE>`, `--check` |
| `task list [OPTIONS]` | Print tasks from the state database — flags: `--status <STATUS>`, `--method <METHOD>`, `--parent <ID>`, `--format <FORMAT>` |
| `task show [OPTIONS] <ID>` | Show details for a specific task — flags: `--format <FORMAT>` |
| `task add [OPTIONS] <TITLE>` | Add a task in `pending` state — flags: `--method <METHOD>`, `--parent <ID>`, `--precondition <PRECONDITION>` |
| `task advance [OPTIONS] <ID>` | Advance the task's subtask pointer |
| `task done [OPTIONS] <ID>` | Mark a task done once its sensor-gated subtasks have passed |
| `task fail [OPTIONS] <ID>` | Mark a task failed |
| `task remove [OPTIONS] <ID>` | Remove or cancel a task |
| `trace add [OPTIONS] --session <SESSION>` | Record a trace of an executed command and its resolution — flags: `--session <SESSION>`, `--task <ID>`, `--command <COMMAND>`, `--error-diff <ERROR_DIFF>`, `--resolution-steps <RESOLUTION_STEPS>` |
| `trace list [OPTIONS] --session <SESSION>` | Print traces for a session — flags: `--session <SESSION>`, `--format <FORMAT>` |
| `trace sessions [OPTIONS]` | List distinct trace session identifiers — flags: `--format <FORMAT>` |
| `distill [OPTIONS]` | Extract a heuristic from a resolved trace into a skill. Review output against the anti-AI-slop checklist (.agents/skills/skill-creator/references/anti_ai_slop.md) — flags: `--skill <SKILL>`, `--pattern <PATTERN>`, `--description <DESCRIPTION>`, `--from-trace <ID>`, `--from-strikes`, `--min-strikes <N>`, `--task <ID>`, `--to-fixture`, `--format <FORMAT>` |
| `errors list [OPTIONS]` | List fail-fast error signatures (prefixed with `sensor:`) — flags: `--task <ID>`, `--scope <SCOPE>`, `--format <FORMAT>` |
| `errors clear [OPTIONS]` | Clear fail-fast error signatures (`--sensor sensor:<name>`) — flags: `--sensor <SENSOR>`, `--task <ID>`, `--scope <SCOPE>`, `--force` |
| `loc [OPTIONS] [PATH]...` | Report line-of-code state for the 500-LOC invariant — flags: `--dir <DIR>`, `--ext <EXT>`, `--format <FORMAT>`, `--warn` |
| `split [OPTIONS] <FILE>` | Extract a large top-level item into a sibling module — flags: `--target <NAME>` |
| `eval [OPTIONS]` | Validate skill structure and benchmark skill evals — flags: `--skill <SKILL>`, `--bless`, `--list-skills`, `--fail-fast`, `--format <FORMAT>`, `--approver <NAME>`, `--no-lift`, `--agent-cmd <COMMAND>`, `--agent-timeout <SECS>`, `--strict-fixtures` |
| `hook install [OPTIONS]` | Write pre-commit and pre-push hooks into `.git/hooks/` — flags: `--force` |
| `hook uninstall [OPTIONS]` | Remove managed hooks, leaving foreign hook files untouched |
| `hook status [OPTIONS]` | Show whether the managed hooks and release binary are present — flags: `--format <FORMAT>` |
| `hook diff [OPTIONS]` | Show diff between installed hooks and current templates |
| `doctor [OPTIONS]` | Run diagnostic checks on binary resolution and git hook health — flags: `--format <FORMAT>`, `--strict` |
| `metrics [OPTIONS] [COMMAND]` | Report harness trends: sensor stats, strikes, eval pass-rate history — flags: `--format <FORMAT>`, `--sensor <SENSOR>`, `--skill <SKILL>`, `--since <UNIX_SECONDS>`, `--scope <SCOPE>`, `--task <ID>`, `--branch <BRANCH>`, `--all` |
| `metrics pr [OPTIONS] --repo <OWNER/NAME>` | PR-loop measures: time-to-green, pushes per green, waivers, comments, cancelled runs, reruns — flags: `--repo <OWNER/NAME>`, `--since <WINDOW>`, `--limit <N>`, `--recompute`, `--format <FORMAT>` |
| `learn [OPTIONS] --draft` | Draft skill/guide updates from sensors that keep firing (never applies them) — flags: `--draft`, `--days <DAYS>`, `--min-fires <N>`, `--format <FORMAT>` |
| `overlap [OPTIONS]` | Rank skill pairs by guidance overlap (Tier-2 distinctiveness advisory) — flags: `--threshold <THRESHOLD>`, `--format <FORMAT>` |
| `maintenance [OPTIONS]` | Prune old beats and compact the state database — flags: `--prune-beats <DAYS>`, `--keep-per-task <N>` |
| `compliance [OPTIONS]` | Print compliance mapping to OWASP Agentic Top 10, NIST AI RMF, and EU AI Act — flags: `--framework <FRAMEWORK>`, `--format <FORMAT>` |
| `audit-chain [OPTIONS]` | Recompute workflow event hash chain and report first divergence — flags: `--format <FORMAT>` |
| `dora [OPTIONS]` | Derive DORA deployment metrics from git history (deterministic, read-only) — flags: `--days <DAYS>`, `--format <FORMAT>`, `--record`, `--source <SOURCE>`, `--now <UNIX_SECONDS>` |
| `ci-explain [OPTIONS] <RUN_ID>` | Explain a CI run's failures and cancellations with local repro commands (alias: `ci`) — flags: `--format <FORMAT>` |
| `completions [OPTIONS] <SHELL>` | Generate shell completions |
| `man [OPTIONS] <DIR>` | Generate man page documentation |

Global flags: `--root <DIR>`, `--config <FILE>`, `-q, --quiet`, `--output <FILE>`, `--dry-run`.
<!-- END GENERATED: cli-commands -->

`--dry-run` previews every mutating `task` action and writes nothing.

## Exit codes

| Code | Meaning |
|------|---------|
| 0 | All sensors passed |
| 1 | A sensor failed |
| 2 | Usage/config/discovery error (e.g. not inside a do-harness workspace, missing git repo for `hook install`) |

## JSON output

`verify --format json` emits a single JSON object:

<!-- example: id=readme_json_report classification=parser-check-only reason="JSON verification output structure" -->
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

- **pre-commit**: runs `verify --fail-fast --record --only fmt --only loc`
- **commit-msg**: runs `scripts/check-commitlint.sh --message` against the prepared message (fail-closed: blocks the commit when the repo or script is missing)
- **pre-push**: runs the full `verify` suite

Managed hooks carry a marker identifying them as do-harness-owned; re-running `hook install` overwrites them, and `--force` overwrites any pre-existing hook at the same path. `hook uninstall` removes only managed hooks. `hook status` reports which hooks are present and whether they are managed.

Alternative for users of the pre-commit framework: this repo ships
`.pre-commit-config.yaml` with a `local` hook (`language: system`) that builds
the CLI and runs the same `verify --fail-fast --only fmt --only loc` gate. It
resolves the binary through `DO_HARNESS_BIN` and otherwise through
`CARGO_TARGET_DIR`'s release directory, so a redirected target directory cannot
make it execute a stale binary. The check itself is already gated in CI by the
`feedback` set, and the managed hook's execution is covered end to end, so this
integration adds a second entry point rather than a second gate.

## Skills

Skills (`.agents/skills/`) are the harness's feedforward guides — markdown
plus executable evals that agents load before coding, complementing the
feedback sensors that check the result. See
[docs/skills.md](https://github.com/d-o-hub/do-harness/blob/main/docs/skills.md)
for the catalog, the progressive-disclosure loading contract (`skills
suggest` → inspect metadata → select → read one body), the authoring loop
(`skill-creator`: scaffold → evals → `eval --strict-fixtures` → `--bless`),
and the one-way sync rule (templates mirror `.agents/skills/`; edit the
canonical copy).

## CI

The offline [workflow effectiveness benchmark](docs/workflow-benchmarks.md)
checks a versioned synthetic corpus and measurement contract. External agent
comparisons are explicit, bounded, and advisory; fixture replay is not evidence
that a skill improves an agent.

GitHub Actions runs three cadences as separate gates. Per pull request,
`.github/workflows/verify.yml` lints shell sensors, builds `do-harness`,
dogfoods `init && verify` on fresh rust/generic workspaces, runs `do-harness
eval --strict-fixtures`, and runs `verify --set verification --format json
--strict` — the set whose `check`/`test`/`check-agt`/`check-mcp` sensors cover
the adjacent `guardian-proxy` crate with default features and with its optional
features off and on — then asserts `status --set verification`. Nightly,
`.github/workflows/scheduled.yml` runs `verify --set scheduled --format json
--strict` over the work no diff can invalidate (the `powerset` feature matrix
and the instrumented `coverage` run) and asserts `status --set scheduled`. Tag
pushes run `verify --set release --format json --strict` in
`.github/workflows/release.yml`. Which sensors belong to which set is maintained
in `do-harness.toml` (`[signal-sets]`), not copied here.

> **Runtime proxy note:** `do-harness` itself stays a dev-loop harness (no traffic proxy). The adjacent `crates/guardian-proxy` is an *optional* fail-closed sidecar (off by default; governance via `agt-governance`, MCP Streamable HTTP ingress via `mcp-surface`) and the only tool-call mediation surface, via its `ProxyMediator` gate — see `crates/guardian-proxy/README.md` when present.

## DeepSeek Harness integration

`integrations/deepseek-harness/` ships an out-of-tree DSH bundle that exposes
do-harness development signals as one typed `development_signals` tool and an
optional strict completion gate. The Rust core has no DeepSeek, Cordis, or
Node.js dependency, and removing the bundle leaves normal CLI use unaffected.
All verification policy stays in the CLI; the bundle only invokes it through
DSH's managed subprocess seam. See
[`integrations/deepseek-harness/README.md`](https://github.com/d-o-hub/do-harness/blob/main/integrations/deepseek-harness/README.md)
for the install walkthrough and configuration.

GitLab CI:

<!-- example: id=readme_gitlab_ci classification=illustrative reason="GitLab CI configuration file snippet" -->
```yaml
verify:
  image: rust:latest
  script:
    - cargo build --release -p do-harness
    - ./target/release/do-harness verify --format json
```

## Compliance positioning

`do-harness` enforces deterministic, computational controls over the agent development loop (build-time verification sensors, workflow gates, tamper-evident logs, and machine-readable evidence artifacts). It is designed to satisfy dev-loop assurance requirements in regulated environments without over-claiming runtime policy or proxy capabilities.

See [docs/compliance.md](https://github.com/d-o-hub/do-harness/blob/main/docs/compliance.md) for full mappings against the **OWASP Agentic Top 10 (2026)**, **NIST AI RMF 1.0**, the **EU AI Act**, and **SOC 2**.

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
