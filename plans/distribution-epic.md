# Epic: Distribution and External Adoption

> **Status:** released as `v0.1.0` (2026-09-13); `v0.1.1` (2026-09-15) ships the first Windows zip (crates.io published all three crates at `0.1.1`; npm publishes four Linux/macOS platform packages plus the `do-harness` meta package, all at `0.1.1` — Windows is GitHub-release-only because npm rejects the `do-harness-win32-x64` name). The meta bootstrap and its Trusted Publisher landed 2026-09-17, so CI publishes through OIDC and `npx do-harness` resolves; see the two defect records below)
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

crates.io publication is **complete for `v0.1.1`**: `do-harness-types`,
`do-harness-db`, and `do-harness` are all published at `0.1.1` (verified
against the registry API on 2026-09-16 — each returns `200` with `0.1.1` in its
version list, not yanked, and a resolvable download path), and
`cargo install do-harness --version 0.1.1` installs a binary reporting
`0.1.1`. The release run's `publish` job packaged and uploaded them in
dependency order; its idempotency guard would have skipped anything already
present. `CARGO_REGISTRY_TOKEN` is therefore configured, and the earlier
"blocked on the secret" note is obsolete.

npm bootstrap publication used the authenticated `d-o-hub` account:
`do-harness-linux-x64`, `do-harness-linux-arm64`, `do-harness-darwin-x64`, and
`do-harness-darwin-arm64` are published at `0.1.1`. npm rejected
`do-harness-win32-x64@0.1.1` with HTTP 403 (`Package name triggered spam
detection`), so the meta package was not published because its pinned Windows
optional dependency is unavailable.

### Decision — Windows ships as a GitHub release, not npm

**Windows has no npm channel, and that is the shipped state.** The
`do-harness-win32-x64` name is refused by npm's registry screening, so the
canonical distribution for Windows is the release zip
(`do-harness-v<version>-x86_64-pc-windows-msvc.zip`, present in `v0.1.1` with a
`checksums.txt` digest), used directly or through `install.sh` under Git Bash.
Source and binstall channels also work. This is documented as a supported
platform path, not a pending gap:

- `docs/adoption.md` (Windows section) and `README.md` state the npm exclusion
  and the working channels.
- `docs/releasing.md` records it in One-time setup and the install-channel table.
- `integrations/npm/lib/platform.js` lists the package in
  `UNAVAILABLE_PACKAGES`, and the shim exits 1 with release guidance instead of
  the misleading `--include=optional` hint.
- `scripts/publish-npm.sh` lists it in `UNAVAILABLE_PKGS`, skips it with an
  explicit log line, and withholds the meta package. Before that list, the
  publish loop aborted on the registry 403, so every tag push produced a red
  `npm-publish` job (the `v0.1.1` run shows exactly this) even though the
  release itself was correct. The job now exits 0 and its log states the
  decision; a genuine failure (a missing artifact, an invalid version) still
  fails the run.
- `integrations/npm/test/shim.test.mjs` pins that guidance with a
  host-independent test, verified to fail when the branch is removed.

Two facts that make the documented failure mode precise:

1. **An unpublished optional dependency is not a publish blocker.** `npm
   publish --dry-run` succeeds for a package whose `optionalDependencies`
   name an absent version (verified directly), so the earlier "meta package is
   withheld" framing overstated the constraint. npm also installs such a
   package cleanly.
2. **The real failure is at run time.** Because install succeeds and the
   binary is missing, `npx do-harness` on Windows fails when the shim runs.
   That is why the shim, not the install step, carries the guidance.

The meta package therefore publishes instead of being withheld (`2026-09-16`).
Withholding it cost every Linux/macOS user a working `npx do-harness` — the
primary install path in `README.md` returned 404 — to avoid a Windows message
the shim already prints. Publishing is necessary but not yet sufficient: the
name has never existed on npm, and a Trusted Publisher can only be configured
for an existing package, so the OIDC job returns `404 Not Found - PUT` until a
one-time authenticated bootstrap publish creates `do-harness` on the registry.
The Windows pin is inert: npm filters an `optionalDependency` by its `os`/`cpu`
before fetching it, so a Linux/macOS install never requests it (verified: clean
install and clean `npm ls`), and a Windows install succeeds silently before the
shim exits 1 with release-zip guidance. Keeping the pin means the moment npm
clears the name, Windows users get the package with no further release change.
Revisit the rename only if npm Support refuses the name (Support-first), with a
coordinated rename as the fallback; a future release can add the Windows npm
channel without changing this decision's documentation, since the shim reads
`UNAVAILABLE_PACKAGES`.

If npm clears the name later: bootstrap-publish `do-harness-win32-x64@0.1.1`,
remove it from `UNAVAILABLE_PACKAGES`, then publish `do-harness@0.1.1`. Only
after each package exists can its trusted publisher be configured
(organization/user `d-o-hub`, repository `do-harness`, workflow `release.yml`,
direct publish). Future versions use the GitHub Actions OIDC path; a tag push
is the normal trigger, and `gh workflow run release.yml -f publish=true` is
the idempotent rerun path.

### Defect found — the OIDC job never reached OIDC (2026-09-17)

The `404 Not Found - PUT` above was the *symptom* the runbook already
predicted, but the job had a second, independent defect that would have
survived the bootstrap: **it never attempted the OIDC exchange at all.**
`actions/setup-node` with `registry-url` unconditionally exports
`NODE_AUTH_TOKEN=XXXXX-XXXXX-XXXXX-XXXXX` (a placeholder, not a credential) and
writes `_authToken=${NODE_AUTH_TOKEN}` into a temp `.npmrc`. `publish-npm.sh`
branched on that variable being non-empty, so in CI it took the token path and
skipped the OIDC branch — proven by the job log containing zero occurrences of
the `Using GitHub Actions OIDC trusted publishing` line, and by the literal
dummy string being sent to the registry as Bearer auth.

Resolution: the publisher treats that exact value as unset *and* clears it, so
the action's `.npmrc` interpolates empty. `check-npm-sequence.sh` gained
`check_publish_script_oidc` plus two mutation controls (`bad-placeholder-blind`,
`bad-placeholder-uncleared`), because the pre-existing workflow check could not
see the defect: the token is generated by the action, not written in the YAML.
An npm token was never needed for this project — a token in a job holding
`id-token: write` masks trusted publishing rather than enabling it.

### Bootstrap completed and OIDC proven in CI (2026-09-17)

The meta package was bootstrapped (`do-harness@0.1.1`, published through the
browser 2FA flow: `npm publish --auth-type=web` prints
`https://www.npmjs.com/auth/cli/<uuid>`), then its Trusted Publisher was
configured with `npm trust github do-harness --file release.yml
--repo d-o-hub/do-harness --allow-publish`, which the registry reports as
`createPackage` + `createStagedPackage` for id `f99825f2`.

Verified against the registry, not the exit code:

| Check | Result |
|---|---|
| `GET registry.npmjs.org/do-harness` | `200`, `latest: 0.1.1` |
| `npx do-harness --version` (unpinned) | `do-harness 0.1.1 (32120c3 2026-09-15)` |
| `npx do-harness init` (README path) | exit 0, `Initial verification: GREEN` |
| npm tarball binary vs `v0.1.1` release asset | `sha256 715fa782…` **identical** |
| shim → `lib/platform.js` → platform pkg → binary | resolves and runs; absent platform prints the `--include=optional` guidance |
| full `release.yml` dispatch | `preflight`, 5 builds, `publish`, `npm-publish` all green |

The dispatched run is the decisive proof for the placeholder fix: its job log
contains `Ignoring the setup-node placeholder NODE_AUTH_TOKEN; using OIDC` and
`Using GitHub Actions OIDC trusted publishing` — both lines were absent from
every earlier run, because the job had never reached that branch. Every package
was already at `0.1.1`, so the run correctly skipped each publish
(`is already on npm; skipping`) rather than re-uploading an immutable version.

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
