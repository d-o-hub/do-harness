# Adopting do-harness in an external codebase

`do-harness` is a compiled CLI. Adopting it in any repository — Rust or not —
is: install the binary, run `init`, wire hooks, and point agents at the
generated contract.

## Install

Zero-install with npx (Node 18+, no binary on `PATH` required):

```bash
npx do-harness init
npx do-harness verify --set verification --changed --strict
```

Or install a pinned prebuilt binary:

```bash
# Latest release (tag resolved from the releases/latest redirect).
curl -fsSL https://raw.githubusercontent.com/d-o-hub/do-harness/main/scripts/install.sh | sh

# Pinned and reproducible (recommended for CI and agent instructions).
curl -fsSL https://raw.githubusercontent.com/d-o-hub/do-harness/main/scripts/install.sh \
  | sh -s -- --version v0.1.2
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

Rust toolchains can install from crates.io instead, either building from
source or fetching the release artifact:

```bash
cargo install do-harness --version 0.1.2
cargo binstall do-harness          # prebuilt, no compile
```

### Air-gapped and vendored installs

- Mirror the release assets and pass `--base-url <URL>`; the layout is
  `<base>/<tag>/do-harness-<tag>-<target>.tar.gz` plus `checksums.txt`.
- Or vendor the source and `cargo install --path vendor/do-harness/crates/do-harness`.
  Do **not** build it into the adopting repository's shared `target/`: a
  `cargo clean` deletes the binary and silently breaks installed git hooks.

## Initialize the repository

```bash
cd /path/to/repository
do-harness init                    # detects the stack; rust pack by default
do-harness init --language generic # no built-in sensors (vacuous pass)
do-harness init --language web     # web UI audit pack (viewport, a11y, console, perf, visual, i18n)
do-harness init --language node    # Node/JS/TS pack (typecheck, lint, test, build)
do-harness hook install            # pre-commit, commit-msg, pre-push
do-harness doctor                  # binary resolution, hooks, db skew
```

`init` writes `do-harness.toml`, `AGENTS.md` (including a pinned installer
line for future agents), `plans/invariants.json`, `.agents/skills/`
(`harness` + `skill-creator`), the rust pack's `scripts/check-*.sh` helpers
plus `scripts/check-release-preflight.sh` and its `plans/RELEASING.md`
runbook, `.gitignore` entries, and the local libSQL state. Existing files are never overwritten without `--force`, and existing
application source is never touched. The run ends with
`Initial verification: GREEN | RED | VACUOUS`; RED exits non-zero.

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
be read.

## The agent loop

Agents (and CI) only need three commands:

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

```yaml
- name: Install do-harness
  run: |
    curl -fsSL https://raw.githubusercontent.com/d-o-hub/do-harness/main/scripts/install.sh \
      | sh -s -- --version v0.1.2
    echo "$HOME/.local/bin" >> "$GITHUB_PATH"
- name: Verify
  run: do-harness verify --set verification --format json --strict
```

GitLab CI:

```yaml
verify:
  script:
    - curl -fsSL https://raw.githubusercontent.com/d-o-hub/do-harness/main/scripts/install.sh | sh -s -- --version v0.1.2
    - export PATH="$HOME/.local/bin:$PATH"
    - do-harness verify --set verification --format json --strict
```

## Releasing

The rust pack scaffolds the release step so it is not re-invented per
repository: a runbook (`plans/RELEASING.md`) and a read-only preflight
(`scripts/check-release-preflight.sh`) that never tags, pushes, or publishes.

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

## Troubleshooting

- `unsupported platform`: install from source, or use a release target above.
- Hooks resolve the binary as `$DO_HARNESS_BIN`, then `do-harness` on `PATH`,
  then `<repo>/target/release/do-harness`. Set `DO_HARNESS_BIN` for custom
  locations (e.g. an npx-managed copy or a version manager shim).
- `do-harness doctor --strict` fails when the binary lives under `target/` or
  a managed hook is missing.
