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
  | sh -s -- --version v0.6.0
```

The installer places `do-harness` in `$HOME/.local/bin` by default. Override
with `--bin-dir <DIR>` or `DO_HARNESS_INSTALL_DIR`; pin with `--version` or
`DO_HARNESS_VERSION`. It verifies the artifact's SHA-256 against the release
`checksums.txt` before installing. Checksums share the release origin, so they
detect corruption, not a compromised origin; verify them out of band when that
matters.

Supported platforms: Linux x86_64/aarch64 (static musl), macOS x86_64/arm64,
and Windows x86_64. **Windows npm installs download the release binary on
first run** — see below.
Other platforms install from source: `cargo install --path crates/do-harness`
(Rust 1.85+).

### Windows

Prebuilt Windows x86_64 (`x86_64-pc-windows-msvc`) binaries ship as a zip
release asset in `v0.1.1`. The initial `v0.1.0` release published Linux/macOS
tarballs only. `install.sh` detects Git Bash (`MINGW*`/`MSYS*`/
`CYGWIN*`) and installs `do-harness.exe` from the zip (needs `unzip` or `7z`
to extract). The `windows-latest` CI job proves the installer (including the
zip path), `hook install`/`status`, and `doctor` on every push.

**Windows via npm downloads the release binary.** The `do-harness-win32-x64`
platform package is rejected by the npm registry's name screening (HTTP 403,
"Package name triggered spam detection"), so the wrapper's pinned platform
package never resolves. Instead, the first `npx do-harness ...` run on
Windows x64:

- fetches `checksums.txt` and
  `do-harness-v<version>-x86_64-pc-windows-msvc.zip` from the GitHub release
  matching the wrapper's own version;
- verifies the zip's SHA-256 against its `checksums.txt` entry and fails
  closed on a mismatch or a missing entry — nothing is executed;
- extracts `do-harness.exe` and caches it under
  `%LOCALAPPDATA%\do-harness\do-harness\v<version>\`;
- reuses the verified cache on later runs (no re-download).

There is no `postinstall` script: nothing runs at install time, so
`npm install do-harness` still just **succeeds** on Windows (an absent
optional dependency is not an install error) — the fetch happens at the first
invocation instead. Set `DO_HARNESS_NO_DOWNLOAD=1` to disable the fetch and
get the release-zip guidance below. If you prefer a fully offline install, or
the fetch is unavailable, the release zip remains supported: either the
installer above, or download
`do-harness-v<version>-x86_64-pc-windows-msvc.zip` plus `checksums.txt` and
unzip `do-harness.exe` yourself.

Source builds on Windows need: the Visual Studio Build Tools C++ workload
(MSVC `link.exe` + Windows SDK libraries) and LLVM/Clang (`libclang` for the
`libsql` → `bindgen` → `clang-sys` chain). With Git Bash, ensure `link.exe`
resolves to MSVC, not coreutils `/usr/bin/link` (which fails with a confusing
"extra operand" error) — check with `command -v link` / `where link`.

Sensor portability: the shipped rust-pack helpers (`loc`, `deps`, `audit`,
`commitlint`) are native `do-harness` subcommands — `do-harness loc --strict`
for the ceiling gate and `do-harness helpers deps|audit|commitlint` for the
dependency, audit, and commit-message checks — so they run without Git Bash or
WSL on Windows. **Wire the native command as the sensor argv; do not name
`scripts/check-*.sh`.** Those files remain as thin delegating wrappers so an
existing config keeps working, but they are a POSIX-shell dependency: on a host
whose only `bash` is the WSL launcher the wrapper fails before the check runs.
`do-harness doctor` reports a shell-routed helper and prints the native command
to switch to; treat that warning as a defect in the adoption, not as advice.
Managed git hooks are POSIX `sh` bodies (no bash-isms) that call the
`do-harness` binary directly; git-for-windows executes hooks through its
bundled MSYS `sh`, so the hooks themselves need no Git Bash user shell (the
binary resolves via `DO_HARNESS_BIN`, `PATH`, then
`target/release/do-harness[.exe]`).

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
do-harness init --language python  # Python pack (ruff lint/format, optional typecheck, pytest)
do-harness init --language go      # Go pack (gofmt, go vet/test, optional staticcheck)
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
all language packs (`rust`, `generic`, `web`, `node`, `python`, `go`). Every pack also gets
`scripts/check-commitlint.sh`, because the managed `commit-msg` hook runs it and
fails closed when it is absent. Existing policy files are never overwritten
without `--force`, and existing application source is never touched. Note that policy
prose specifies behavioral expectations and human approval boundaries, but does not claim
technical runtime enforcement over tool execution. The run ends with
`Initial verification: GREEN | INCOMPLETE | RED | VACUOUS`; RED exits non-zero.
`INCOMPLETE` means every generated sensor passed but a core one (`fmt`,
`check`, `clippy`, `test`) was omitted for missing tooling — for example
`clippy` without the clippy component — so the pass is not full evidence.
Install the tool and re-run `init --force` to add the sensor. The `test`
sensor is never omitted for a missing `cargo-nextest`: it runs
`scripts/check-test.sh`, which prefers nextest and falls back to `cargo test`
with its own zero-test guard.


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

The harness supports six language packs: `rust` (Rust sensor pack),
`generic` (no built-in sensors), `web` (web UI audit pack), `node`
(Node/JS/TS pack probing package scripts and toolchains), `python` (ruff
lint/format, optional typecheck, pytest), and `go` (gofmt, go vet/test,
optional staticcheck). Plain `init` selects the Node pack automatically when
a `package.json` proves a supported command (a `typecheck`/`lint`/`test`/
`build` script, turbo task, or matching dependency); Python markers
(`pyproject.toml`, `requirements.txt`, `setup.py`, `setup.cfg`, `tox.ini`)
select the Python pack; a `go.mod` selects the Go pack. A malformed or
unsupported manifest stays Generic with a structured diagnostic
(`invalid_package_json`, `no_supported_node_commands`,
`missing_package_manager`, `missing_python_tooling`, `missing_go_tooling`).
The generic pack ships **zero sensors**: its pass is vacuous, not evidence.
Add real checks before trusting `verify`.

## The adopting-repository sensor contract

Language packs are convenience presets over one stack-neutral contract:
`[[sensors]]` in `do-harness.toml`. Any repository can declare the commands it
already trusts, and `init` writing configuration is not the same as verifying
the codebase. The elements below are the contract every pack renders:

| Element | Contract |
| --- | --- |
| Command | `argv` is a program followed by arguments, executed without a shell. Path-looking arguments resolve against the workspace root; a missing path fails the sensor instead of recording a pass. |
| Working directory | Every sensor runs with `cwd` set to the workspace root (the `--root` directory), never the sensor's own directory or a subdirectory. |
| Timeout | `timeout` is seconds per attempt (default 3600). `timeout = 0` requires `timeout-justification` and means unbounded; prefer a finite value. |
| Required vs advisory | The default `severity = "error"` is required. `severity = "warn"` and `kind = "project-check"` are advisory; `fix` is printed as remediation when they fail. `verify --strict` promotes advisory failures except in the `feedback` loop. |
| Bounded diagnostics | `max-output-bytes` defaults to 1 MiB and is capped at 16 MiB. The report retains a bounded tail, and a timeout terminates the process group rather than leaving children running. |
| Applicable inputs | `when-changed` globs choose the sensors for the current change. `inputs`, `coverage-inputs`, and `environment-inputs` declare additional evidence inputs; an input that cannot be resolved fails closed. |
| Exit outcome | `0` = every required sensor passed; `1` = a sensor ran and failed; `2` = usage/config error or every selected sensor was unrunnable. A missing program is `unrunnable`, never a pass. |
| Evidence freshness | `verify --set <set> --strict --record` writes the evidence artifact; `status --set <set>` reports `green`, `red`, `stale`, or `missing` without running sensors. Freshness is bound to inputs, policy, selection range, and harness identity. |

A run that selects no sensors is `VACUOUS`: the JSON report sets
`vacuous: true` and text output says the run is not verification evidence. A
zero-sensor exit code of `0` never means the repository was verified.

## Cross-stack command recipes

These recipes use the generic pack and the repository's existing commands.
`init` auto-detects Rust, Node, Python, and Go; the entries below are the
manual generic form for when you prefer to hand-declare sensors (or adapt an
existing task runner) after `init`.

### Rust

The Rust pack generates these sensors, including the zero-test guard in
`scripts/check-test.sh`. Use the same shape when adapting an existing pack:

<!-- example: id=adoption_contract_rust classification=parser-check-only reason="Rust sensor contract recipe" -->
```toml
[[sensors]]
name = "check"
argv = ["cargo", "check", "--workspace"]
when-changed = ["**/*.rs", "Cargo.toml", "**/Cargo.toml", "Cargo.lock"]

[[sensors]]
name = "test"
argv = ["bash", "scripts/check-test.sh"]
when-changed = ["**/*.rs", "Cargo.toml", "**/Cargo.toml", "Cargo.lock"]
```

### Node/TypeScript

Prefer the `package.json` scripts the project already maintains. The Node pack
probes them; this is the equivalent generic form:

<!-- example: id=adoption_contract_node classification=parser-check-only reason="Node/TypeScript sensor contract recipe" -->
```toml
[[sensors]]
name = "test"
argv = ["npm", "test"]
when-changed = ["src/**", "test/**", "package.json", "package-lock.json"]

[[sensors]]
name = "typecheck"
argv = ["npm", "run", "typecheck"]
when-changed = ["src/**", "package.json", "tsconfig.json"]
```

### Python

`init` auto-detects the Python pack from `pyproject.toml`/`requirements.txt`/
`setup.py`/`setup.cfg`/`tox.ini` and generates `ruff check`, `ruff format
--check`, an optional typecheck (mypy/pyright, only when configured), and a
`pytest` test sensor with a zero-tests-collected guard. `pytest` is a useful
default because it exits non-zero when it collects zero tests; the pack's test
sensor treats that as a skip. If the project uses `unittest`, wrap `python3 -m
unittest discover` in a guard that fails on `Ran 0 tests`. The manual generic
form is:

<!-- example: id=adoption_contract_python classification=parser-check-only reason="Python sensor contract recipe" -->
```toml
[[sensors]]
name = "test"
argv = ["python3", "-m", "pytest", "-q"]
when-changed = ["**/*.py", "pyproject.toml", "requirements.txt", "setup.cfg", "tox.ini"]

[[sensors]]
name = "lint"
argv = ["python3", "-m", "ruff", "check", "."]
severity = "warn"
when-changed = ["**/*.py", "pyproject.toml", "ruff.toml"]
```

### Go

`init` auto-detects the Go pack from `go.mod` and generates `gofmt -l`,
`go vet ./...`, `go test ./...`, and an optional `staticcheck` when installed.
`go test ./...` exits `0` when a package has no test files, so pair a manual
`test` sensor with a guard that fails on `[no test files]` (or ensure every
tested package has a test). `go vet` is a useful required or advisory
companion. The manual generic form is:

<!-- example: id=adoption_contract_go classification=parser-check-only reason="Go sensor contract recipe" -->
```toml
[[sensors]]
name = "vet"
argv = ["go", "vet", "./..."]
when-changed = ["**/*.go", "go.mod", "go.sum"]

[[sensors]]
name = "test"
argv = ["bash", "scripts/check-go-test.sh"]
when-changed = ["**/*.go", "go.mod", "go.sum"]
```

<!-- example: id=adoption_contract_go_guard classification=illustrative reason="Go zero-test guard wrapper" -->
```bash
#!/usr/bin/env bash
set -euo pipefail
out="$(go test ./... 2>&1)"
printf '%s\n' "$out"
! grep -q '\[no test files\]' <<<"$out"
```

## SDK packages

For agent frameworks that want a typed interface instead of shelling out to
the CLI, the repository ships two lightweight SDK packages under
[`integrations/sdk-python/README.md`](../integrations/sdk-python/README.md) and
[`integrations/sdk-ts/README.md`](../integrations/sdk-ts/README.md):

- **Python** — `do-harness-sdk` (PyPI): standard-library-only dataclass
  models; wraps `verify`, `status`, `explain`, `errors list`, `task list`,
  `task export`, and `version`. Includes an optional LangChain `Tool` adapter
  (`pip install 'do-harness-sdk[langchain]'`).
- **TypeScript** — `do-harness-sdk` (npm): zero-dependency ESM with
  TypeScript declarations; same command surface.

Both SDKs share the same contract:

- They spawn the harness binary with argv arrays (never a shell) and parse
  its `--format json` documents into typed models.
- They fail closed: a missing binary, unparseable output, or a non-zero exit
  raises an SDK error (red `verify`/`status` runs raise typed errors that
  carry the parsed report).
- Their package versions mirror the CLI release they target.
  `HarnessClient.assert_schema_compatible()` / `assertSchemaCompatible()`
  verify the installed CLI's release line.

The offline `sdk-pack` gate builds both SDKs and checks that their package
versions match the CLI. The tagged release workflow can publish those same
artifacts through [SDK trusted publishing](releasing.md#sdk-publishing).

## Onboarding outcomes

`init` reports one baseline state per run. Configuration is written before the
baseline executes, so a red result still leaves an editable contract.

| Situation | Pack | Baseline | Exit | Meaning |
| --- | --- | --- | --- | --- |
| Unsupported `package.json` | `generic` | `VACUOUS` | 0 | `no_supported_node_commands` diagnostic; add a script or declare `[[sensors]]`. |
| Unknown stack | `generic` | `VACUOUS` | 0 | `generic_stack_selected` diagnostic names the markers; use the recipes above. |
| No sensors declared | `generic` | `VACUOUS` | 0 | Nothing ran; this is not evidence. |
| Missing Node package manager | `node` | `INCOMPLETE` | 0 | `missing_package_manager`; install it and re-run `init --force`. |
| Missing Python tooling | `python` | `INCOMPLETE` | 0 | `missing_python_tooling`; install `ruff`/`pytest` and re-run `init --force`. |
| Missing Go tooling | `go` | `INCOMPLETE` | 0 | `missing_go_tooling`; install the Go toolchain and re-run `init --force`. |
| Missing Rust core tool | `rust` | `INCOMPLETE` | 0 | The omitted sensor is named; install the tool and re-run `init --force`. |
| Missing declared tool at verify | any | n/a | 2 | `verify` exits 2 and names the sensor in `unrunnable`; it is never a pass. |
| Existing red baseline | any | `RED` | 1 | The repository's own checks failed; the generated config is still written. |

Exit `0` means “configuration written and no required sensor failed”. Verified
means `baseline.state == "green"` **and** `do-harness status --set verification`
is green for the current inputs.

## What init owns and preserves

- Files are written only when absent; `--force` overwrites. Existing
  application source, build manifests, and the greenfield crate scaffold are
  never replaced.
- `.gitignore` is appended to, never rewritten; an existing entry is skipped.
- `init` does not install or edit git hooks. `hook install` owns the
  pre-commit, commit-msg, and pre-push hooks and refuses a foreign hook without
  `--force`. A hook delegated to `pre-commit` or a configured `core.hooksPath`
  is reported as delegated, not replaced.
- The 500-LOC `loc` ceiling is a Rust-pack default; other packs opt in
  explicitly. The commit-message check is `scripts/check-commitlint.sh`, which
  the optional `commit-msg` hook runs from **this repository**; replace the
  script to use your own convention. Neither is a prerequisite for using the
  harness.

## Configure sensors for your stack

Review the [sensor execution trust boundary and resource policy](sensor-execution.md)
before running third-party configuration or checks from untrusted pull requests.

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
change (`verify --changed` for the working tree, `verify --since <ref>` for the
committed range `merge-base(<ref>, HEAD)..HEAD`), and selection fails closed
when git state cannot be read. Path filtering selects which sensors run; it
does not prove the change is semantically independent, so a matched sensor
still has to pass. Providing a non-empty `[[sensors]]` list replaces the built-in sensor
pack entirely. Non-empty `pre-commit` and `pre-push` hook selections under
`[hooks]` are validated against the effective sensor pack (`effective_sensors`),
and any unknown sensor name fails configuration loading before hooks are
written. An empty hook list defaults to running the full effective suite.

`do-harness.toml` carries an explicit `schema-version`; `init` writes the
current one. `do-harness migrate --diff` previews the changes a newer binary
needs (currently: declare the version and replace the deprecated
`allow_failure` alias with `severity = "warn"`), and `migrate --write` applies
them, re-parsing the result with the real config parser. Ambiguous blocks and
files from a newer binary are refused, never guessed at; `doctor` fails on
either skew and names the fix.

`sensor new` writes that entry for you and then proves it: the block is
appended, an existing single-line signal-set array gains the name with
`--set`, and the resulting configuration is resolved through the same selection
path `explain`/`verify` use. A failed proof restores the file byte for byte.
`--script` scaffolds an executable stub that exits non-zero until you replace
its body, so a placeholder can never pass as a check.

<!-- example: id=adoption_sensor_new classification=illustrative reason="Sensor scaffolding command" -->
```bash
do-harness sensor new ruff --arg ruff --arg check --arg . \
  --when-changed '**/*.py' --set feedback --script scripts/ruff.sh
do-harness verify --only ruff
```

`verify` resolves the path-looking arguments in each sensor's `argv` before it
spawns the command. A missing script or data file fails the sensor instead of
recording a pass, because an interpreter or wrapper can exit 0 while ignoring
the missing path. `doctor` reports the same miss as a warning, or as a failure
under `--strict`. Flags, values, shell command strings passed to `-c`, URLs and
git refs are not treated as paths.

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
argv = ["do-harness", "loc", "--strict", "--dir", "web,src,crates", "--ext", "rs,ts,tsx"]
when-changed = ["**/*.rs", "web/**/*.ts", "web/**/*.tsx"]
```

Run the native gate, never `scripts/check-loc.sh`: the shell wrapper is a
POSIX dependency, and a host whose only `bash` is the WSL launcher fails before
measuring anything. `--dir`/`--ext` accept repeats or comma-separated lists,
the ceiling stays 500 and the decomposition threshold 450, generated trees
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

### Wiring the pack: `do-harness setup web`

`do-harness setup web` bootstraps the peer dependencies and the environment in
one step. It detects the package manager from the lockfile (`pnpm-lock.yaml`,
`yarn.lock`, `bun.lockb`/`bun.lock`, else `package-lock.json`/npm), checks
that `playwright` (>= 1.40), `@axe-core/playwright` (>= 4.8), and
`lighthouse` (>= 11.0) are installed at compatible versions, offers to install
what is missing (interactive prompt on a TTY; pass `--yes`/`-y` to skip it),
and appends a `[env]` table to `do-harness.toml` with the
`WEB_AUDIT_ROUTES` placeholder and the documented defaults. `[env]` values are
applied to every sensor process, so the web sensors stop SKIPping once the
dependencies are installed.

Non-interactive runs without `--yes` print the exact install command and exit
2 instead of prompting or installing; `--dry-run` reports what would change
without writing. The `[env]` block is written even when dependencies remain
missing, so the pack is configured as soon as they are installed.

After setup, confirm the pack with:

<!-- example: id=adoption_setup_web_confirm classification=illustrative reason="Post-setup web pack confirmation command" -->
```bash
do-harness verify --only viewport-ux,a11y,console,perf,visual,i18n,web-ui-tests
```

See [`integrations/web-ui/README.md`](../integrations/web-ui/README.md) for the
audit stages, matrix contract, and visual-baseline semantics.

## The agent loop

Use the [workflow benchmark](workflow-benchmarks.md) to compare a pinned skill
or policy against a baseline with identical inputs and budgets. Start with its
offline deterministic checks; external agent evaluation is an explicit opt-in.

Agents (and CI) only need three commands:

<!-- example: id=adoption_agent_loop classification=illustrative reason="Agent edit loop CLI commands" -->
```bash
do-harness verify --set feedback --changed          # during edits
do-harness verify --set verification --changed --strict   # before completion
do-harness verify --set verification --since origin/main --strict  # clean committed branch
do-harness status --set verification                # evidence freshness (no run)
do-harness task context <ID> --format json          # bounded resume handoff (read-only)
```

`verify --format json` emits one JSON object on stdout (failure tails go to
stderr), so it is safe to parse; its `selection` object names the mode, the
resolved base/head/merge-base SHAs, the driving paths, and every skip reason.
Its `unrunnable` array names sensors that never started (an unspawnable
program, an empty `argv`, or a missing declared `argv` path); when every
selected sensor is unrunnable the run exits `2` instead of `1`.

### Planning a run before paying for it

`verify --simulate` is the pre-flight an agent or a CI scheduler runs first: it
resolves the same selection `verify` would use and prints the plan — `argv`,
signal sets, `when-changed` globs, timeout, retry, severity, and skip reasons —
then exits `0` without spawning a sensor, reading beat state, or writing
evidence. `--record`, `--bless`, and `--evidence` are rejected, and JUnit is not
a plan format.

<!-- example: id=adoption_simulate classification=illustrative reason="Simulate pre-flight CLI command" -->
```bash
do-harness verify --set verification --changed --simulate --format json
```

The document carries `schema_version: 1`; `sensors` is the run order and
`estimated_order` mirrors the names for consumers that only need the schedule.

### Reports a CI system can ingest

`verify --format junit` emits a JUnit XML suite with one `<testcase>` per
sensor: a failing `error`-severity sensor is `<failure>` with its already
redacted output tail, an unrunnable one is `<error>` (the run still exits `2`),
and a reused, planned, or advisory result is `<skipped>` so a green advisory
run never shows a red test. A vacuous run (no sensors selected) is an `<error>`
testcase, because an empty suite would otherwise read as a pass. Exit codes are
unchanged.

`status --format json` returns
`green | red | stale | missing` without running sensors; `green` means passing
evidence still matches the workspace, policy, and selection revisions (a moved
head or advanced base reports `stale` with reason `selection_changed`).
`verify --set <set>` writes
`.do-harness/evidence.<set>.json`, so feedback runs never clobber verification
evidence.

### Selecting against the CI base branch

`--since @ci` resolves the pull/merge request's base branch from the provider's
own variable (`GITHUB_BASE_REF`, `CI_MERGE_REQUEST_TARGET_BRANCH_NAME`,
`SYSTEM_PULLREQUEST_TARGETBRANCH`, `BITBUCKET_PR_DESTINATION_BRANCH`,
`BUILDKITE_PULL_REQUEST_BASE_BRANCH`, `CHANGE_TARGET`,
`CI_COMMIT_TARGET_BRANCH`) and selects `merge-base(origin/<base>, HEAD)..HEAD`.
It is opt-in and fail-closed: no known variable, or a base branch that was never
fetched, exits `2` with the fetch command to run — never a green full run. The
evidence records the requested spelling (`@ci`) and the resolved revisions, so
`status --since @ci` and `status --since origin/<base>` agree or report
`selection_changed`. A shallow checkout needs `fetch-depth: 0` (or
`git fetch origin <base>`).

<!-- example: id=adoption_since_ci classification=illustrative reason="CI base resolution preflight" -->
```bash
do-harness explain --since @ci --format json     # resolves with no sensor run
do-harness verify --set verification --since @ci --strict
```

On session start or resume, `task context <ID>` assembles the current step,
required sensors, task-scoped freshness and failures, recent traces, and one
next command into a single bounded result. It is a projection of recorded
facts, not a memory service: it cannot supply product acceptance criteria the
task record does not carry, and a complete-looking context packet is never
evidence that the task is done. Completion still requires the sensor gates
(`task done`) or an explicit human decision.

### Multi-workspace verification

For a monorepo with independently configured packages, pass a root-relative
glob. The root policy is validated, then each matched directory loads its own
`do-harness.toml`, runs from that directory, and writes any recorded beats or
evidence under its own `.do-harness/` state. The glob may match a workspace
directory or its `do-harness.toml`; generated directories are pruned.

`--jobs` bounds concurrent workspace processes (default: `1`). When set, each
workspace runs its sensors sequentially so the total process limit is explicit.
Without it, each workspace keeps its configured sensor concurrency. `--changed`
and `--since` compute one change set at the monorepo root, scope it to each
workspace, and report unaffected workspaces as skipped. An empty global change
set still exits `2`; a root or workspace policy-file change runs all selected
sensors in the affected workspaces.

`--output` and `--evidence` paths are relative to each workspace; absolute paths
and `..` escapes are rejected. Each JSON result includes the aggregate verdict,
exit code, workspace status, and the nested per-workspace verify report.
`--config` is not accepted because each workspace supplies its own config;
`--format junit` is not supported for aggregate reports.

<!-- example: id=adoption_workspaces classification=illustrative reason="Multi-workspace verification invocation" -->
```bash
do-harness verify --workspaces 'packages/*' --jobs 4 --set verification --changed --strict --format json
```

## CI snippets

GitHub Actions:

<!-- example: id=adoption_gha_yaml classification=illustrative reason="GitHub Actions YAML workflow snippet" -->
```yaml
- name: Install do-harness
  run: |
    curl -fsSL https://raw.githubusercontent.com/d-o-hub/do-harness/main/scripts/install.sh \
      | sh -s -- --version v0.6.0
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
    - curl -fsSL https://raw.githubusercontent.com/d-o-hub/do-harness/main/scripts/install.sh | sh -s -- --version v0.6.0
    - export PATH="$HOME/.local/bin:$PATH"
    - export CI=true DO_HARNESS_REQUIRE_TOOLS=1
    - do-harness verify --set verification --format json --strict
```

Set `CI=true` and `DO_HARNESS_REQUIRE_TOOLS=1` (the repo's own
`.github/workflows/verify.yml` does this): without them `check-deps.sh` /
`check-audit.sh` print `SKIP` when `cargo-deny`/`cargo-audit` are absent, and
since a SKIP records as `warn`, a strict run fails on an otherwise clean repo.

For a CI test tab, run the same gate with `--format junit` and publish the XML.
Exit codes do not change, so the job still fails on a red sensor; the file only
adds inline annotations and a test report. A failing sensor carries its redacted
output tail, an unrunnable one is an `<error>`, and an advisory result is
`<skipped>`.

<!-- example: id=adoption_gha_junit classification=illustrative reason="GitHub Actions JUnit upload" -->
```yaml
- name: Verify
  env:
    CI: "true"
    DO_HARNESS_REQUIRE_TOOLS: "1"
  run: do-harness verify --set verification --format junit --strict > junit.xml
- name: Publish test report
  if: always()
  uses: actions/upload-artifact@v4
  with:
    name: verify-junit
    path: junit.xml
```

<!-- example: id=adoption_gitlab_junit classification=illustrative reason="GitLab JUnit artifact" -->
```yaml
verify:
  script:
    - do-harness verify --set verification --format junit --strict > junit.xml
  artifacts:
    when: always
    reports:
      junit: junit.xml
```

## Sensor execution reports

`verify --record` persists one beat per sensor run. `report` exports them
historically: durations, exit codes, status, and scope, filtered by an inclusive
Unix-seconds window (`--since`, `--until`), `--sensor`, and the same scope
selectors `metrics` uses (`--scope`, `--task`, `--branch`, `--all`). It reads
the state database only; it never runs a sensor.

`--format json` emits `schema_version: 1` with `scope`, `window`, and an
`executions` array (oldest first). `--format prometheus` emits valid text
exposition for a scrape, aggregated per `(sensor, scope)` over the window:
`do_harness_sensor_runs_total`, `do_harness_sensor_failures_total`,
`do_harness_sensor_duration_seconds_total`, and
`do_harness_sensor_last_exit_code`. A window-scoped counter resets when the
window moves, so scrape the whole history for monotonically increasing totals.

<!-- example: id=adoption_report classification=illustrative reason="Report export commands" -->
```bash
do-harness report --format json --since 1760000000 --scope branch:main
do-harness report --format prometheus --all
```

GitHub Actions job summary from the JSON export (`jq` is preinstalled):

<!-- example: id=adoption_report_gha classification=illustrative reason="GitHub Actions job summary from report JSON" -->
```yaml
- name: Sensor history
  if: always()
  run: |
    do-harness report --format json --all \
      | jq -r '"| sensor | runs |", "|---|---|", (.executions | group_by(.sensor)[] | "| \(.[0].sensor) | \(length) |")' \
      >> "$GITHUB_STEP_SUMMARY"
```

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

The harness enforces strict fail-closed contract rules across all supported packs (`rust`, `generic`, `node`, `web`, `python`, `go`):

| Pack | Selection Criteria | Default Generated Sensors | Strict Verification Contract |
|------|-------------------|--------------------------|------------------------------|
| `rust` | `Cargo.toml` | `fmt`, `check`, `clippy`, `test`, `doctest`, `loc`, `deps`, `audit`, `commitlint`, `release-preflight` | Fails closed on zero tests, broken doctests, missing manifest, or missing required tools in CI. |
| `generic` | Greenfield / unknown stack or malformed/unsupported `package.json` | None (empty sensor array) | Vacuous pass (`sensor_pack = "generic"`, 0 sensors executed) with an actionable diagnostic. Add the cross-stack recipes above; never masquerades as strict evidence for a specific language gate. |
| `node` | `package.json` proving a supported Node command (script, turbo task, or matching dependency) | `typecheck`, `lint`, `test`, `build` | Preserves `package.json` and `src/`. Probes package scripts, turbo, tsc, eslint, jest, vitest. Missing package-manager tooling yields an INCOMPLETE baseline, never a claimed green; `test` fails when the runner's total summary reports zero tests, so a delegated script cannot pass vacuously. |
| `python` | Python markers (`pyproject.toml`, `requirements.txt`, `setup.py`, `setup.cfg`, `tox.ini`) | `lint`, `format`, optional `typecheck`, `test` | Probes `ruff`, the configured type checker, and `pytest`. A sensor is included only when its tooling probe succeeds; missing `pytest` (or a missing configured checker) yields an INCOMPLETE baseline, never a claimed green. The `test` sensor runs through `scripts/check-python-test.sh`, which treats pytest's zero-tests-collected exit as a skip. |
| `go` | `go.mod` | `fmt`, `vet`, `test`, optional `staticcheck` | Probes `gofmt`, `go`, and optional `staticcheck`. Missing `go`/`gofmt` yields an INCOMPLETE baseline, never a claimed green. |
| `web` | `init --language web` | `web-ui-tests`, `viewport-ux`, `a11y`, `console`, `perf`, `visual`, `i18n` | Unconfigured pack emits `SKIP:` (warned verdict). Fails closed under `verify --strict` until `WEB_AUDIT_ROUTES` is set. `web-ui-tests` runs the bundled audit suite and needs no configuration. |

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
- `state database has migration N, newer than this binary knows (max M)`:
  the `do-harness` on `PATH` is older than the workspace state. Upgrade it
  (`npm install -g do-harness@latest`, `cargo install do-harness`) or point
  `DO_HARNESS_BIN` at the newer build; `do-harness doctor` reports the same
  skew before a hook blocks a commit.
- `do-harness doctor --strict` fails when the binary lives under `target/` or
  a hook is missing and not delegated. A hook is delegated when its executable
  file is a pre-commit wrapper, when `.pre-commit-config.yaml` declares
  pre-commit as the owner, or when it is a foreign hook under a configured
  `core.hooksPath`; an empty or non-executable file is never delegated, because
  git does not run it. `hook install` refuses to overwrite such a wrapper
  without `--force`.
