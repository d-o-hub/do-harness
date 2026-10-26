# Adopting do-harness in an external codebase

`do-harness` is a compiled CLI. Adopting it in any repository — Rust or not —
is: install the binary, run `init`, wire hooks, and point agents at the
generated contract.

## Install

Zero-install with npx (Node 18+, no binary on `PATH` required):

<!-- example: id=adoption_npx classification=illustrative reason="npx runner invocation" -->
```bash
npx do-harness init
npx do-harness verify --set verification --changed --strict
```

Or install a pinned prebuilt binary:

<!-- example: id=adoption_install classification=executable fixture=installer_local_artifact -->
```bash
# Latest release (tag resolved from the releases/latest redirect).
curl -fsSL https://raw.githubusercontent.com/d-o-hub/do-harness/main/scripts/install.sh | sh

# Pinned and reproducible (recommended for CI and agent instructions).
curl -fsSL https://raw.githubusercontent.com/d-o-hub/do-harness/main/scripts/install.sh \
  | sh -s -- --version v0.2.1
```

The installer places `do-harness` in `$HOME/.local/bin` by default. Override
with `--bin-dir <DIR>` or `DO_HARNESS_INSTALL_DIR`; pin with `--version` or
`DO_HARNESS_VERSION`. It verifies the artifact's SHA-256 against the release
`checksums.txt` before installing. Checksums share the release origin, so they
detect corruption, not a compromised origin; verify them out of band when that
matters.

Supported platforms: Linux x86_64/aarch64 (static musl), macOS x86_64/arm64,
and Windows x86_64. **Windows is not installable through npm** — see below.
Other platforms install from source: `cargo install --path crates/do-harness`
(Rust 1.85+).

### Windows

Prebuilt Windows x86_64 (`x86_64-pc-windows-msvc`) binaries ship as a zip
release asset in `v0.1.1`. The initial `v0.1.0` release published Linux/macOS
tarballs only. `install.sh` detects Git Bash (`MINGW*`/`MSYS*`/
`CYGWIN*`) and installs `do-harness.exe` from the zip (needs `unzip` or `7z`
to extract). The `windows-latest` CI job proves the installer (including the
zip path), `hook install`/`status`, and `doctor` on every push.

**Windows via npm is unavailable.** The `do-harness-win32-x64` platform
package is rejected by the npm registry's name screening (HTTP 403, "Package
name triggered spam detection"), so `npx do-harness` and
`npm install do-harness` cannot resolve a Windows binary. Two consequences
worth knowing, because neither is obvious:

- `npm install do-harness` still **succeeds** on Windows. An absent optional
  dependency is not an install error, so the meta package installs and then
  fails at run time with guidance pointing at the release zip.
- The channel that works is a GitHub release: either the installer above, or
  download `do-harness-v<version>-x86_64-pc-windows-msvc.zip` plus
  `checksums.txt` and unzip `do-harness.exe` yourself.

Source builds on Windows need: the Visual Studio Build Tools C++ workload
(MSVC `link.exe` + Windows SDK libraries) and LLVM/Clang (`libclang` for the
`libsql` → `bindgen` → `clang-sys` chain). With Git Bash, ensure `link.exe`
resolves to MSVC, not coreutils `/usr/bin/link` (which fails with a confusing
"extra operand" error) — check with `command -v link` / `where link`.

Sensor portability: `[[sensors]]` `argv` is shell-agnostic, but the shipped
rust-pack helpers are POSIX shell scripts — run hooks and dogfood under Git
Bash on Windows. Managed git hooks resolve `do-harness.exe` via the `.exe`
fallback.

Rust toolchains can fetch the prebuilt release artifact:

<!-- example: id=adoption_binstall classification=illustrative reason="cargo binstall tool" -->
```bash
cargo binstall do-harness          # prebuilt, no compile
```

Registry source installs through 0.2.1 lose the local libSQL teardown patch.
From 0.3.0 on — published, with registry validation on musl and Windows — use
`cargo install do-harness --version 0.3.0 --locked`; the normalized dependency
names the maintained `do-harness-libsql` fork. `--locked` makes version selection
reproducible, but cannot add a missing source fix to an older release, so a
0.2.1-or-earlier install wants the tested prebuilt artifacts.

### Air-gapped and vendored installs

- Mirror the release assets and pass `--base-url <URL>`; the layout is
  `<base>/<tag>/do-harness-<tag>-<target>.tar.gz` plus `checksums.txt`.
- Or vendor the source and `cargo install --path vendor/do-harness/crates/do-harness`.
  Do **not** build it into the adopting repository's shared `target/`: a
  `cargo clean` deletes the binary and silently breaks installed git hooks.

## Initialize the repository

<!-- example: id=adoption_init_flow classification=illustrative reason="Multi-command adoption walkthrough" -->
```bash
cd /path/to/repository
do-harness init                    # detects the pack from the repository
do-harness init --language rust    # Rust sensor pack, explicit
do-harness init --language generic # no built-in sensors (vacuous pass)
do-harness init --language web     # web UI audit pack (viewport, a11y, console, perf, visual, i18n)
do-harness init --language node    # Node/JS/TS pack (typecheck, lint, test, build)
do-harness hook install            # pre-commit, commit-msg, pre-push
do-harness doctor                  # binary resolution, hooks, db skew
```

`init` writes `do-harness.toml`, `AGENTS.md` (including a pinned installer
line for future agents), `USE_RESTRICTIONS.md` (standalone agent safety policy
defining high-risk boundaries and human approval limits), `plans/invariants.json`,
`.agents/skills/` (`harness` + `skill-creator`), the rust pack's `scripts/check-*.sh`
helpers plus `scripts/check-release-preflight.sh` and its `plans/RELEASING.md`
runbook, `.gitignore` entries, the ignored `.agents/events/` metrics-event
tree, and the local libSQL state. Scaffolding of `USE_RESTRICTIONS.md` is independent
from skill scaffolding (it is written even under `--minimal`) and is emitted across
all language packs (`rust`, `generic`, `web`, `node`). Existing policy files are never overwritten
without `--force`, and existing application source is never touched. Note that policy
prose specifies behavioral expectations and human approval boundaries, but does not claim
technical runtime enforcement over tool execution. The run ends with
`Initial verification: GREEN | RED | VACUOUS`; RED exits non-zero.


### Refreshing the skill-creator validator

Repositories created before a validator update keep their local
`.agents/skills/skill-creator/scripts/quick_validate.py`: `do-harness eval`
copies that local file into its sandbox, and ordinary `init` leaves it
untouched. For skills using top-level `version`/`category` or eval fixtures
with a top-level `version`, first upgrade the CLI to **v0.3.5 or newer** so
the evaluator understands the fixture schema, then run:

<!-- example: id=adoption_refresh_validator classification=illustrative reason="Targeted validator refresh for existing repositories" -->
```bash
do-harness skills refresh-validator
```

This replaces only `quick_validate.py`; it preserves custom skill instructions,
references, and other files. Do not use `init --force` for this refresh.

A repository adopted before it has any test ends `RED` on the `test` sensor:
the pack does not pass `--no-tests=pass`, so an empty suite is reported by
name instead of recording a pass that checked nothing. Add a test, or drop the
sensor from `do-harness.toml` — the run writes that contract first.

### Optional: verify a context sanitizer

`init` writes `scripts/check-context-sanitization.py` and an inert, commented
recipe in the generated `do-harness.toml`. The sensor is **opt-in**: it is not
registered by default, no default signal set names it, and an installed
sanitizer binary never enables it implicitly.

The harness does not mediate context. It never captures prompts, tool
arguments, or vault contents. The check proves one narrow thing — that a
configured sanitizer transforms a synthetic fixture — and nothing more. That
evidence is **capability evidence about the fixture**, not proof that a real
coding client routes every outbound byte through the sanitizer; that claim
needs a client-specific integration test.

Activate it in two steps. First, declare the sensor and add its name to the
signal sets your policy requires:

<!-- example: id=adoption_context_sanitization_toml classification=parser-check-only reason="Sample context-sanitization sensor TOML snippet" -->
```toml
[[sensors]]
name = "context-sanitization"
argv = ["python3", "scripts/check-context-sanitization.py"]
severity = "error"
timeout = 45
```

Then run it with the two environment variables it reads:

<!-- example: id=adoption_context_sanitization_run classification=parser-check-only reason="Sample context-sanitization invocation" -->
```bash
DO_HARNESS_CONTEXT_SANITIZATION=enabled \
DO_CONTEXT_SHIELD_BIN=/absolute/path/to/do-context-shield \
  do-harness verify --only context-sanitization --strict
```

`DO_HARNESS_CONTEXT_SANITIZATION` is `disabled` or `enabled`; unset means
`disabled`. `DO_CONTEXT_SHIELD_BIN` names the executable and is required only
when the mode is `enabled` — a missing or non-executable path is a hard
failure, not a skip.

The disabled mode prints `SKIP: context-sanitization: not configured`. Under
`--strict` that skip is **missing verification, not success**: a policy that
requires this sensor must not report green while it is disabled.

For client registration, see the upstream
[client integration guide](https://github.com/d-oit/do-context-shield/blob/main/docs/client-integration.md).
The harness deliberately does not duplicate the MCP protocol details.

To require evidence for a particular client's outbound boundary, use the
separate [client conformance contract](client-conformance.md). Guidance
installation, a passing sanitizer fixture, and a verified client boundary
are separate claims; unsupported routes remain unknown.

`plans/invariants.json` is a JSON array of `DecisionHeader` objects
(`{invariant, rationale, sensor, category}`), or equivalently an object with
a top-level `invariants` array (extra keys such as `$comment` are ignored).
Each header stays strict: unknown fields inside a header are rejected.

The harness supports four language packs: `rust` (Rust sensor pack),
`generic` (no built-in sensors), `web` (web UI audit pack), and `node`
(Node/JS/TS pack probing package scripts and toolchains). The generic pack
ships **zero sensors**: its pass is vacuous, not evidence. Add real checks
before trusting `verify`.

## Configure sensors for your stack

<!-- example: id=adoption_toml_config classification=parser-check-only reason="Sample do-harness.toml configuration fragment" -->
```toml
# do-harness.toml
language = "generic"

[signal-sets]
feedback = ["typecheck"]
verification = ["typecheck", "test"]
release = ["typecheck", "test", "build"]

[[sensors]]
name = "typecheck"
argv = ["npm", "run", "typecheck"]
when-changed = ["src/**", "package.json", "tsconfig.json"]

[[sensors]]
name = "test"
argv = ["npm", "test", "--", "--runInBand"]
when-changed = ["src/**", "test/**"]
```

A sensor is one `argv` command; a signal set is a decision composed from
sensor names. `when-changed` globs make a sensor apply only to the current
change (`verify --changed`), and selection fails closed when git state cannot
be read. Providing a non-empty `[[sensors]]` list replaces the built-in sensor
pack entirely. Non-empty `pre-commit` and `pre-push` hook selections under
`[hooks]` are validated against the effective sensor pack (`effective_sensors`),
and any unknown sensor name fails configuration loading before hooks are
written. An empty hook list defaults to running the full effective suite.

Status documents rot silently, so `init` also scaffolds a reference
`project-check`: `scripts/check-tracker-drift.sh` compares `<n> open PRs` /
`<n> open issues` claims in a status document with `gh pr list` / `gh issue
list`, prints the drift with its line number, and fails closed when it cannot
verify a claim. Declare it (soft by default) to enable it:

<!-- example: id=adoption_tracker_drift_toml classification=parser-check-only reason="Sample project-check TOML snippet" -->
```toml
[[sensors]]
name = "tracker-drift"
kind = "project-check"
argv = ["bash", "scripts/check-tracker-drift.sh", "--doc", "plans/ROADMAP_ACTIVE.md"]
fix = "refresh the open-PR/issue counts in the status document"
when-changed = ["plans/**/*.md", "scripts/check-tracker-drift.sh"]
```

`kind = "project-check"` is the general form: any `argv` whose failure is
advisory. It defaults to `severity = "warn"`, and `fix` is printed as a `FIX:`
line when the check fails.

The LOC ceiling is not Rust-only: the rust pack ships `loc` over `*.rs` by
default, and a front-end tree gets the same invariant by scoping that sensor
instead of forking the script.

A workspace member manifest is a compilation input: a dependency, a feature,
or a lint-policy edit in `crates/db/Cargo.toml` changes what the compiler and
the test runner see without touching a `.rs` file or the lockfile. The rust
pack and the repository policy therefore list both `Cargo.toml` and
`**/Cargo.toml` in the `when-changed` globs of the `fmt`, `check`, `clippy`,
`test`, `doctest`, and `coverage` sensors.

**An existing adopter has to make this correction itself.** Installing a new
binary does not rewrite an adopter's `do-harness.toml`: the file is the
adopter's policy, and the harness reads it as written. Until the recursive
pattern is added, `verify --changed` can skip compilation, tests, and doctests
after a member-manifest edit. Add the pattern by hand:

<!-- example: id=adoption_member_manifest_toml classification=parser-check-only reason="Sample member-manifest applicability TOML snippet" -->
```toml
[[sensors]]
name = "check"
argv = ["cargo", "check", "--workspace"]
when-changed = ["**/*.rs", "Cargo.toml", "**/Cargo.toml", "Cargo.lock"]
```

<!-- example: id=adoption_loc_toml classification=parser-check-only reason="Sample loc sensor TOML snippet" -->
```toml
[[sensors]]
name = "loc"
argv = ["bash", "scripts/check-loc.sh", "--root", "web,src,crates", "--ext", "rs,ts,tsx"]
when-changed = ["**/*.rs", "web/**/*.ts", "web/**/*.tsx"]
```

`--root`/`--ext` accept repeats or comma-separated lists, `--max`/`--warn` move
the ceiling (500) and the split threshold (450), generated trees
(`node_modules`, `target`, `dist`, `build`, …) are always pruned, and a
configured root that does not exist fails the run instead of passing vacuously. See
`docs/cli.md` for the full flag list; the `FINDINGS: <n>` contract that
`verify --record --bless` pins is unchanged.

## Web pack configuration

`init --language web` scaffolds the web UI audit pack (viewport text, axe
accessibility, console noise, performance, visual diffs, i18n). The generated
sensors **SKIP until configured**: each needs `WEB_AUDIT_ROUTES` and a running
app at `WEB_AUDIT_BASE_URL`. A SKIP is recorded as `warn`, so `verify --strict`
fails on an unconfigured web pack instead of passing vacuously.

| Env var | Purpose |
|---------|---------|
| `WEB_AUDIT_ROUTES` | Comma-separated app routes to audit (required; unset ⇒ SKIP). |
| `WEB_AUDIT_BASE_URL` | Base URL the sensors navigate (default `http://127.0.0.1:3000`). |
| `WEB_AUDIT_LOCALES` | Locales for the i18n audit, baseline first (fewer than two ⇒ SKIP). |
| `WEB_AUDIT_LOCALE_PARAM` | Query parameter used to switch locale (default `lang`). |
| `WEB_AUDIT_LOCALE_COOKIE` | Cookie name used to switch locale instead of the query parameter. |
| `WEB_PERF_BUDGETS` | JSON perf budgets, keys `performanceScore`/`lcpMs`/`cls`/`tbtMs`. |
| `WEB_VISUAL_BASELINE_DIR` | Visual baseline PNG directory (default `.do-harness/visual`). |
| `WEB_VISUAL_UPDATE` | Set to `1` to re-bless visual baselines (only after verifying the change is intended). |

Peer dependencies must be installed in the adopting repository's `node_modules`
(a missing one makes the corresponding sensor SKIP): `playwright`,
`@axe-core/playwright` (a11y), and `lighthouse` (perf).

See [`integrations/web-ui/README.md`](../integrations/web-ui/README.md) for the
audit stages, matrix contract, and visual-baseline semantics.

## The agent loop

Agents (and CI) only need three commands:

<!-- example: id=adoption_agent_loop classification=illustrative reason="Agent edit loop CLI commands" -->
```bash
do-harness verify --set feedback --changed          # during edits
do-harness verify --set verification --changed --strict   # before completion
do-harness status --set verification                # evidence freshness (no run)
```

`verify --format json` emits one JSON object on stdout (failure tails go to
stderr), so it is safe to parse. `status --format json` returns
`green | red | stale | missing` without running sensors; `green` means passing
evidence still matches the workspace and policy. `verify --set <set>` writes
`.do-harness/evidence.<set>.json`, so feedback runs never clobber verification
evidence.

## CI snippets

GitHub Actions:

<!-- example: id=adoption_gha_yaml classification=illustrative reason="GitHub Actions YAML workflow snippet" -->
```yaml
- name: Install do-harness
  run: |
    curl -fsSL https://raw.githubusercontent.com/d-o-hub/do-harness/main/scripts/install.sh \
      | sh -s -- --version v0.2.1
    echo "$HOME/.local/bin" >> "$GITHUB_PATH"
- name: Verify
  env:
    CI: "true"
    DO_HARNESS_REQUIRE_TOOLS: "1"
  run: do-harness verify --set verification --format json --strict
```

GitLab CI:

<!-- example: id=adoption_gitlab_yaml classification=illustrative reason="GitLab CI YAML workflow snippet" -->
```yaml
verify:
  script:
    - curl -fsSL https://raw.githubusercontent.com/d-o-hub/do-harness/main/scripts/install.sh | sh -s -- --version v0.2.1
    - export PATH="$HOME/.local/bin:$PATH"
    - export CI=true DO_HARNESS_REQUIRE_TOOLS=1
    - do-harness verify --set verification --format json --strict
```

Set `CI=true` and `DO_HARNESS_REQUIRE_TOOLS=1` (the repo's own
`.github/workflows/verify.yml` does this): without them `check-deps.sh` /
`check-audit.sh` print `SKIP` when `cargo-deny`/`cargo-audit` are absent, and
since a SKIP records as `warn`, a strict run fails on an otherwise clean repo.

## Releasing

The rust pack scaffolds the release step so it is not re-invented per
repository: a runbook (`plans/RELEASING.md`) and a read-only preflight
(`scripts/check-release-preflight.sh`) that never tags, pushes, or publishes.

<!-- example: id=adoption_release_preflight classification=illustrative reason="Release preflight script execution" -->
```bash
bash scripts/check-release-preflight.sh            # pins agree (offline)
bash scripts/check-release-preflight.sh --release  # + target not already published
```

The preflight reads `VERSION`, `Cargo.toml` (`[workspace.package]` first, then
`[package]`) and every tracked, non-private `package.json`; a mismatch names
every pin that disagrees. A manifest's own top-level `version` is the pin: a
`version` key inside a dependency, override, or `publishConfig` object is not
one. `--release` lists the published releases through
`gh` (repository from `GH_REPO` or the `origin` remote) and fails with
`already has a GitHub Release` when the target already shipped — the failure
mode behind doomed release dispatches. It prints `WARN` and exits 0 when `gh`
or the network is unavailable, unless `CI=true` or
`DO_HARNESS_REQUIRE_TOOLS=1`, where a skipped guard fails instead. A manifest
whose version is stamped at publish time is not a pin: mark the package
`"private": true`, or pass `--no-package-json` (adding it to the generated
sensor's `argv`) to keep the `VERSION`/`Cargo.toml` check without it.

`init` puts the offline half in the generated config, so the dev loop never
needs the network:

<!-- example: id=adoption_release_signal_sets classification=parser-check-only reason="Release signal sets configuration TOML" -->
```toml
[signal-sets]
# release-preflight also joins `verification`; it is offline, so neither set
# needs the network.
release = ["fmt", "check", "clippy", "test", "doctest", "coverage", "loc", "deps", "audit", "commitlint", "release-preflight"]

[[sensors]]
name = "release-preflight"
argv = ["bash", "scripts/check-release-preflight.sh"]
when-changed = ["VERSION", "Cargo.toml", "**/Cargo.toml", "**/package.json"]
```

Editing a pin re-runs the sensor under `verify --changed`, and
`do-harness verify --set release` runs it with everything else before a tag;
the published-release comparison stays in the runbook's release step, because
between releases the committed version legitimately equals the last released
one. Repositories initialized before this scaffold can copy the script and
runbook and add the block above. do-harness's own tag-triggered process is
documented in `docs/releasing.md`.

## Adoption contract matrix

The harness enforces strict fail-closed contract rules across all supported packs (`rust`, `generic`, `node`, `web`):

| Pack | Selection Criteria | Default Generated Sensors | Strict Verification Contract |
|------|-------------------|--------------------------|------------------------------|
| `rust` | `Cargo.toml` | `fmt`, `check`, `clippy`, `test`, `doctest`, `loc`, `deps`, `audit`, `commitlint`, `release-preflight` | Fails closed on zero tests, broken doctests, missing manifest, or missing required tools in CI. |
| `generic` | Greenfield / non-Rust default | None (empty sensor array) | Vacuous pass (`sensor_pack = "generic"`, 0 sensors executed). Never masquerades as strict evidence for a specific language gate. |
| `node` | `package.json` | `typecheck`, `lint`, `test`, `build` | Preserves `package.json` and `src/`. Probes package scripts, turbo, tsc, eslint, jest, vitest. |
| `web` | `init --language web` | `viewport-ux`, `a11y`, `console`, `perf`, `visual`, `i18n` | Unconfigured pack emits `SKIP:` (warned verdict). Fails closed under `verify --strict` until `WEB_AUDIT_ROUTES` is set. |

### Fail-closed verification rules

1. **Advisory Local vs Required CI Behavior**:
   - Locally, missing optional tools (e.g. `cargo-audit`, `cargo-deny`, `actionlint`) print `SKIP:` and record as `warn` so local development is not blocked.
   - Under `CI=true` or `DO_HARNESS_REQUIRE_TOOLS=1`, missing tools or skipped checks emit `FAIL:` and exit non-zero. `verify --strict` rejects all weak or warned evidence.
2. **Missing Declared Artifacts**:
   - If a sensor defines `artifacts = ["dist/*.js"]` and the command succeeds without generating matching artifacts, `verify --strict` rejects the run with weak evidence (`verdict: fail`).
3. **Mid-Run Input Mutation & Stale Evidence**:
   - If source inputs or `do-harness.toml` configuration change during a run or post-execution, `status` reports `state = "red"` (`inputs_changed_during_run`).
4. **Configuration Validation**:
   - Dangling sensor references in `[signal-sets]`, `[hooks]` (`pre-commit` or `pre-push`), unknown sensor names, or invalid TOML produce exit code 2 with actionable diagnostic output naming the failing setting.
5. **Non-Destructive Adoption**:
   - `init` never overwrites existing application code or configuration files without explicit `--force`.

## Troubleshooting

- `unsupported platform`: install from source, or use a release target above.
- Hooks resolve the binary as `$DO_HARNESS_BIN`, then `do-harness` on `PATH`,
  then `<repo>/target/release/do-harness`. Set `DO_HARNESS_BIN` for custom
  locations (e.g. an npx-managed copy or a version manager shim).
- `do-harness doctor --strict` fails when the binary lives under `target/` or
  a managed hook is missing.
