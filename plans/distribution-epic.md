# Epic: Distribution and External Adoption

> **Status:** released as `v0.1.0` (2026-09-13); `v0.1.1` (2026-09-15) ships the first Windows zip (crates.io publish pending `CARGO_REGISTRY_TOKEN`; npm bootstrap published four platform packages, while `do-harness-win32-x64@0.1.1` is blocked by npm spam detection and the meta package is withheld)
> **Related:** prebuilt releases, `scripts/install.sh`, pinned agent
> instructions, release evidence
> **Created:** 2026-09-13

## Why

Adopting `do-harness` in an external codebase required vendoring the source
and a Rust toolchain (`cargo install --path`), which taxes every non-Rust
repository an agent works in. There was no release workflow, no install
script, and no agent-facing acquisition path. This epic makes the CLI
installable with one pinned command on Linux and macOS, and makes the
generated agent contract self-bootstrapping.

## Slices

| Slice | Exit criteria | Task |
|-------|---------------|------|
| Release workflow | `v*` tag builds Linux/macOS x86_64+aarch64 artifacts, dogfoods each binary, publishes tarballs + `checksums.txt`; tag must equal the workspace version; dispatch is a dry run | 40 |
| Installer | `scripts/install.sh` resolves latest or a pinned tag, verifies SHA-256 from `checksums.txt`, installs atomically to a configurable bin dir, and rejects a tampered artifact | 41 |
| Agent instructions | `do-harness init` writes `AGENTS.md` with the pinned installer command for its own version; README + `docs/adoption.md` cover per-ecosystem quickstarts and CI | 42 |
| Release evidence | CI runs the installer end-to-end; `SECURITY.md` states the release support policy and provenance | 43 |

## Boundaries

- **do-harness owns:** the binary, its release artifacts, the installer, and
  the generated contract. Release policy stays in this repository.
- **Adopting repository owns:** its sensor configuration, hooks, CI wiring,
  and version pin.
- **The installer owns:** download + checksum + placement only. It never
  edits the adopting repository or runs `init`.

## Evidence

- Spike (2026-09-13): `x86_64-unknown-linux-musl` builds a static-pie binary
  that passes greenfield `init && verify` (8/8 sensors).
- `scripts/test-install.sh` passes locally: verified install, version match,
  and tampered-artifact rejection with a checksum-mismatch diagnostic.
- CI: `verify.yml` builds the release binary and runs the installer e2e.
- Release: `release.yml` preflight re-verifies the tagged commit with the full
  verification set. The first `v0.1.0` run failed on every target because the
  matrix dogfood ran `init` without `cargo-deny`/`cargo-audit`, whose sensors
  fail closed under `CI=true`; PR #60 installed both tools and the re-tagged
  run published four tarballs + `checksums.txt` (release job gated on all
  builds passing).
- Published release verified end-to-end with the real installer: pinned
  (`--version v0.1.0`) and latest-resolution installs both produced
  `do-harness 0.1.0 (3e04dd2)`, and the installed binary passed
  `init && verify --set verification --strict` (8/8 sensors).

## Follow-up: crates.io and cargo-binstall

| Slice | Exit criteria | Task |
|-------|---------------|------|
| Packageable assets | The CLI crate embeds `docs/compliance.md` and `plans/methods.json` through symlinked assets; the greenfield scaffold is not a nested package; `cargo package -p do-harness` verifies | 44 |
| Registry metadata | `repository`/`keywords`/`categories` on all three crates, symlinked README/LICENSE, and `[package.metadata.binstall]` with QuickInstall disabled | 45 |
| Publish job + docs | Tag-gated, dependency-ordered, idempotent crates.io publish job with index-propagation retries; `docs/releasing.md`; README/adoption install options | 46 |

Evidence: `cargo publish --dry-run -p do-harness-types` uploads (dry run);
`cargo package` verifies `do-harness-db` and `do-harness` with local patches
for the not-yet-published deps; `cargo-binstall 1.23.0 binstall do-harness
--manifest-path crates/do-harness/Cargo.toml --dry-run` resolved the real
`v0.1.0` `x86_64-unknown-linux-musl` asset and binary path from GitHub.

## Follow-up: zero-install npm wrapper

| Slice | Exit criteria | Task |
|-------|---------------|------|
| Wrapper packages | Meta package with pinned platform `optionalDependencies`, a stdio-inheriting shim, and five platform package templates | 47 |
| Publish assembly | `scripts/publish-npm.sh` stages platform packages from release artifacts, patches versions, skips published versions, and supports a tokenless `--dry-run` | 48 |
| Release wiring | Tag-gated `npm-publish` job (platform-first, then meta) plus shim tests and a dry-run assembly step in CI; docs list `npx do-harness` | 49 |

Evidence: `node --test integrations/npm/test/*.test.mjs` (mapping, arg
passthrough, exit-code propagation, missing/unsupported-platform guidance);
`scripts/publish-npm.sh --dist <dir> --dry-run` assembles all five packages;
a real local `npm install` of packed meta + platform tarballs ran
`node_modules/.bin/do-harness version` and `npx --no-install do-harness
version` against the release binary.

## Follow-up: Windows x86_64

| Slice | Exit criteria | Task |
|-------|---------------|------|
| Release artifact | `x86_64-pc-windows-msvc` builds on `windows-latest`, dogfoods `init && verify` under Git Bash, and ships as a zip in the release + checksums | 51 |
| Hooks and npm | Hook fallback resolves `do-harness.exe`; `win32-x64` platform package + shim `.exe` handling; publish assembly accepts the zip | 52 |
| CI and docs | `windows-latest` job runs workspace tests, dogfood, and the npm shim tests; docs list the Windows install paths | 53 |

Evidence: local `cargo check -p do-harness --target x86_64-pc-windows-gnu`
green; hook unit test asserts the `.exe` fallback; npm tests assert the
`win32-x64` mapping and `.exe` binary name; the PR's windows job is the
end-to-end proof.

### Published — v0.1.1 (2026-09-15)

`v0.1.1` (tag on `32120c3`, release run
[34993196353](https://github.com/d-o-hub/do-harness/actions/runs/34993196353))
is the first release shipping the Windows x86_64 zip. The published release
carries six assets — the four Linux/macOS tarballs,
`do-harness-v0.1.1-x86_64-pc-windows-msvc.zip`, and `checksums.txt` — and all
five artifacts verify against the published SHA-256 digests (`sha256sum -c
checksums.txt`, all `OK`).

`install.sh --version v0.1.1` verified end-to-end: the Linux install reports
`Installed do-harness v0.1.1 (x86_64-unknown-linux-musl)` and the binary
reports `0.1.1 (32120c3 2026-09-15)`; the zip path
(`DO_HARNESS_TARGET=x86_64-pc-windows-msvc`) extracts a checksum-verified
`do-harness.exe` (PE32+ x86-64).

crates.io publication remains blocked on the one-time
`CARGO_REGISTRY_TOKEN` repository secret. npm bootstrap publication used the
authenticated `d-o-hub` account: `do-harness-linux-x64`,
`do-harness-linux-arm64`, `do-harness-darwin-x64`, and
`do-harness-darwin-arm64` are published at `0.1.1`. npm rejected
`do-harness-win32-x64@0.1.1` with HTTP 403 (`Package name triggered spam
detection`), so the meta package was not published because its pinned Windows
optional dependency is unavailable. File npm support for the exact package
name before retrying; do not rename only one package.

After npm clears the name, configure the trusted publisher for all six
packages (organization/user `d-o-hub`, repository `do-harness`, workflow
`release.yml`, direct publish), then rerun the idempotent publish path through
GitHub Actions with `gh workflow run release.yml -f publish=true`.

## Non-goals

- No npm/npx wrapper; the installer and crates.io are the acquisition paths. (Superseded by the npm-wrapper follow-up.)
- No Windows targets (managed hooks are bash). (Superseded by the Windows x86_64 follow-up.)
- No do-harness MCP wrapper: agents integrate through the scaffolded skills +
  `AGENTS.md` and the CLI's JSON contracts. MCP stays guardian-proxy-only
  (`mcp-surface`) until a concrete MCP-only runtime requires host-side
  execution; completion gating belongs to runtime lifecycle hooks (DSH bundle
  pattern), and an MCP server cannot enforce it. Decision recorded in
  `plans/invariants.json` (sensor: `do-harness eval`).
- No auto-update and no version-bump tooling: releases are tag-driven per
  `docs/releasing.md`; `v0.1.1` shipped the first Windows zip.
