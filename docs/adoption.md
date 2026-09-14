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
  | sh -s -- --version v0.1.0
```

The installer places `do-harness` in `$HOME/.local/bin` by default. Override
with `--bin-dir <DIR>` or `DO_HARNESS_INSTALL_DIR`; pin with `--version` or
`DO_HARNESS_VERSION`. It verifies the artifact's SHA-256 against the release
`checksums.txt` before installing. Checksums share the release origin, so they
detect corruption, not a compromised origin; verify them out of band when that
matters.

Supported platforms: Linux x86_64/aarch64 (static musl), macOS x86_64/arm64,
and Windows x86_64 (zip release, npm, or `cargo install`). Other platforms
install from source: `cargo install --path crates/do-harness` (Rust 1.85+).

### Windows

Prebuilt Windows x86_64 binaries ship as a zip release asset (see the
release workflow); until the first release that includes them, install from
source or run the Linux binary under WSL.

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
cargo install do-harness --version 0.1.0
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
do-harness hook install            # pre-commit, commit-msg, pre-push
do-harness doctor                  # binary resolution, hooks, db skew
```

`init` writes `do-harness.toml`, `AGENTS.md` (including a pinned installer
line for future agents), `plans/invariants.json`, `.agents/skills/`
(`harness` + `skill-creator`), `.gitignore` entries, and the local libSQL
state. Existing files are never overwritten without `--force`, and existing
application source is never touched. The run ends with
`Initial verification: GREEN | RED | VACUOUS`; RED exits non-zero.

`plans/invariants.json` is a JSON array of `DecisionHeader` objects
(`{invariant, rationale, sensor, category}`), or equivalently an object with
a top-level `invariants` array (extra keys such as `$comment` are ignored).
Each header stays strict: unknown fields inside a header are rejected.

The generic pack ships **zero sensors**: its pass is vacuous, not evidence.
Add real checks before trusting `verify`.

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
      | sh -s -- --version v0.1.0
    echo "$HOME/.local/bin" >> "$GITHUB_PATH"
- name: Verify
  run: do-harness verify --set verification --format json --strict
```

GitLab CI:

```yaml
verify:
  script:
    - curl -fsSL https://raw.githubusercontent.com/d-o-hub/do-harness/main/scripts/install.sh | sh -s -- --version v0.1.0
    - export PATH="$HOME/.local/bin:$PATH"
    - do-harness verify --set verification --format json --strict
```

## Troubleshooting

- `unsupported platform`: install from source, or use a release target above.
- Hooks resolve the binary as `$DO_HARNESS_BIN`, then `do-harness` on `PATH`,
  then `<repo>/target/release/do-harness`. Set `DO_HARNESS_BIN` for custom
  locations (e.g. an npx-managed copy or a version manager shim).
- `do-harness doctor --strict` fails when the binary lives under `target/` or
  a managed hook is missing.
