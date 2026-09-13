# Epic: Distribution and External Adoption

> **Status:** released as `v0.1.0` (2026-09-13)
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

## Non-goals

- No crates.io publish or `cargo-binstall` metadata (candidate follow-up).
- No npm/npx wrapper; the installer is the single acquisition path.
- No Windows targets (managed hooks are bash).
- No CLI-native MCP surface; an out-of-tree wrapper can follow the DSH bundle
  pattern.
- No auto-update and no version-bump tooling: the first release is fixed at
  `v0.1.0`.
