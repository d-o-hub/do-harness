# Releasing

Releases are cut from `main` by pushing a `v<version>` tag. The tag must equal
`[workspace.package].version` in `Cargo.toml`; the release preflight fails
otherwise. Nothing publishes unless the tagged commit passes the full
verification set and every build target dogfoods green.

## One-time setup

- **crates.io publishes through Trusted Publishing (OIDC); there is no publish
  secret.** For each published crate — `do-harness-types`, `do-harness-db`,
  `do-harness` — open <https://crates.io> → the crate → **Settings → Trusted
  Publishing** and add a GitHub publisher with repository `d-o-hub/do-harness`
  and workflow filename `release.yml`. Leave *environment* empty: the `publish`
  job declares none, and an environment recorded here but absent from the job
  fails the exchange. The job takes `id-token: write`, exchanges its OIDC token
  with `rust-lang/crates-io-auth-action`, and passes the short-lived token to
  `cargo publish` through `CARGO_REGISTRY_TOKEN` for that step only. A crate
  with no trusted publisher fails the job loudly at the exchange; revoke any
  API token created for the previous secret-based path.
- npm Trusted Publisher setup is per existing package. If a package has not
  been published yet, perform a one-time authenticated bootstrap publish first;
  npm exposes its **Settings → Trusted Publisher** page only afterward.
- **Windows ships through GitHub Releases only, not npm.**
  `do-harness-win32-x64` is rejected by npm's registry name screening (HTTP
  403, "Package name triggered spam detection"), so no Windows package exists
  and `npx do-harness` cannot resolve a binary on Windows. The `v0.1.1` release
  ships `do-harness-v<version>-x86_64-pc-windows-msvc.zip`; that zip (or the
  installer, or `cargo install`) is the Windows install path.
  `integrations/npm/lib/platform.js` records the unavailable package so the
  shim exits with release guidance. The meta package is still published: its
  Windows pin is inert elsewhere (npm filters an `optionalDependency` by
  `os`/`cpu` before fetching) and a Windows `npm install` succeeds silently,
  leaving the shim to print the actionable error. Revisit only if npm clears
  the name; a coordinated rename is the fallback.
- **Bootstrap is done for `do-harness` (2026-09-17).** A package must exist
  before npm exposes its Trusted Publisher, and OIDC cannot create a new name,
  so the first publish of any *new* package needs one authenticated publish.
  Bootstrap publish first, then configure its Trusted Publisher:
  `do-harness@0.1.1` was bootstrapped and configured with
  `npm trust github do-harness --file release.yml --repo d-o-hub/do-harness
  --allow-publish`, so every later release publishes through OIDC. Before that
  bootstrap the CI job failed with `404 Not Found - PUT
  https://registry.npmjs.org/<pkg>` — npm's answer when no trust relationship
  exists for the name, not an authentication error. Do not read that 404 as a
  broken OIDC setup. On a 2FA account the bootstrap takes a one-time password:
  `scripts/publish-npm.sh --dist <dir> --otp <code>` (the code is passed to npm
  as `--otp` and never logged). The browser flow
  (`npm publish --auth-type=web`) also works — it prints
  `https://www.npmjs.com/auth/cli/<uuid>` — but the approval URL expires
  quickly, so prefer `--otp` for a scripted bootstrap.
- **Repeat this for every new package name** (e.g. if the Windows name is ever
  cleared): bootstrap-publish it, then
  `npm trust github <pkg> --file release.yml --repo d-o-hub/do-harness
  --allow-publish`. `npm trust` requires an interactive 2FA challenge and
  writes `createPackage` + `createStagedPackage`; `npm trust list <pkg>`
  verifies what the registry holds.
- **A trusted publisher created after 2026-09-03 allows `npm stage publish`
  only unless direct publishing was granted.** npm's docs: configurations from
  before that date keep their old behaviour, configurations after it default to
  staged publishing, and "Allowed actions" must additionally permit
  `npm publish`. A release therefore fails with
  `403 Forbidden - PUT https://registry.npmjs.org/<pkg> - OIDC permission
  denied for this action` — which reads like a credential problem but is a
  missing permission on the action — until each package's connection is granted
  direct publishing (`npm trust github <pkg> --file release.yml --repo
  d-o-hub/do-harness --allow-publish`, or the package's **Trusted publishing →
  Allowed actions** setting). The grant is per package name and takes an
  interactive 2FA challenge. Measured on the v0.1.2 tag for all five published
  packages.
- **A tag run executes the workflow file from the tagged commit.** A `release.yml`
  fix therefore cannot be validated by re-running or re-pushing the existing tag
  — `gh run rerun` replays the old file identically. After a fix lands on `main`:
  delete any release created from the tag (`gh release delete <tag>`), delete and
  re-create the tag on the fixed commit, and push it. crates.io and npm are
  immutable-but-idempotent per version, so a re-cut skips whatever the failed run
  already published; a crate that has already shipped at that version cannot be
  replaced, only followed by a new version.
- Trusted publishing requires Node >= 22.14.0 and npm >= 11.5.1; the release
  job pins Node 24 and checks both versions. npm generates provenance
  automatically for these public GitHub Actions publishes. After a successful
  migration, revoke unused publish tokens and enable npm's **require
  two-factor authentication and disallow tokens** setting.
  See [`docs/provenance-trust-model.md`](provenance-trust-model.md) for the exact
  trust boundaries and claims proven by provenance artifacts.
- **`actions/setup-node` exports a fake credential, and it silently defeats
  OIDC.** With `registry-url` set, the action always runs
  `core.exportVariable('NODE_AUTH_TOKEN', process.env.NODE_AUTH_TOKEN || 'XXXXX-XXXXX-XXXXX-XXXXX')`
  and writes `//registry.npmjs.org/:_authToken=${NODE_AUTH_TOKEN}` into a temp
  `.npmrc`. Any publisher that reads `NODE_AUTH_TOKEN` and branches on it being
  non-empty sees that placeholder, takes the token path, and never reaches the
  OIDC exchange — while also sending the literal string to the registry as
  Bearer auth (`401 Unauthorized`). `scripts/publish-npm.sh` therefore treats
  this exact value as unset *and* clears it, so the temp `.npmrc` interpolates
  empty. `check-npm-sequence.sh` enforces both halves. Do not "fix" a 401 here
  by setting a token: a token in a job holding `id-token: write` is the defect.
- For `cargo binstall` support, no extra setup is needed: the CLI manifest
  declares `[package.metadata.binstall]`, and the release assets match the
  cargo-binstall defaults (`do-harness-v<version>-<target>.tar.gz` containing
  `do-harness-v<version>-<target>/do-harness`). QuickInstall is disabled on
  supply-chain grounds; `compile` remains as the last-resort strategy.

## Cutting a release

1. Bump `[workspace.package].version` in `Cargo.toml`, run
   `cargo update --workspace` (or `cargo check`) so `Cargo.lock` matches, and
   merge that change to `main`.
2. Tag the merge commit and push the tag:

   ```bash
   git tag v0.2.0 <merge-commit>
   git push origin v0.2.0
   ```

3. The `release` workflow runs:
   - `preflight` — asserts tag == version and runs
     `verify --set verification --format json --strict` on the tagged commit.
   - `build` — Linux static-musl (x86_64/aarch64) and macOS (x86_64/arm64)
     tarballs plus a Windows x86_64 zip, each dogfooded with `init && verify`
     and each attested for build provenance in the job that produced it.
    - `publish` — publishes the three crates to crates.io in dependency order,
      authenticating with a short-lived token exchanged from its OIDC identity.
    - `npm-publish` — publishes the available platform packages, then the
      `do-harness` meta package. It skips `UNAVAILABLE_PKGS` (currently
      `do-harness-win32-x64`, which npm refuses) and **still publishes the meta
      package**: an unavailable pin is inert because npm filters an
      `optionalDependency` by `os`/`cpu` before fetching it, and withholding the
      meta package would break `npx do-harness` for every Linux/macOS user. A
      trusted publisher whose permissions lack `npm publish` fails the job at
      the first package instead, naming the package and the remedy. See
      "Windows has no npm channel" above.
    - `release` — publishes the tarballs, the Windows zip, and `checksums.txt`
      via `gh release create --notes-file`. It runs only after `publish` and
      `npm-publish` succeed, so a Release is never created beside an
      unpublished registry version; if either publication fails, the tag exists
      with no Release and the recovery is to re-run the tag-push workflow (a
      `workflow_dispatch` skips `release`).

### Verifying a download

Two independent properties, and a user can check both:

```bash
gh release download v0.1.3 --repo d-o-hub/do-harness
sha256sum -c checksums.txt                       # integrity
gh attestation verify do-harness-v0.1.3-x86_64-unknown-linux-musl.tar.gz \
  --repo d-o-hub/do-harness                      # origin
```

`checksums.txt` is generated in the `release` job from the artifacts it just
downloaded, so it travels from the same origin as the files it protects: it
detects a corrupt or truncated download, not a substituted asset. The attestation
is created by `actions/attest-build-provenance` inside the matrix job that built
the archive — signed through Sigstore with a token minted for that job, stored
against the artifact's digest, and checked by `gh attestation verify` against this
repository's `release.yml`, its ref, and its commit. The registry channels carry
their own evidence (npm attestations, crates.io Trusted Publishing);
`docs/provenance-trust-model.md` states what each one proves and what it does not.

### Release notes

The `release` job routes every published body through `scripts/release-notes.sh`,
which composes it into `release-notes.md` and then refuses a body that is missing
a section. The shape is fixed:

| Section | Content |
|---|---|
| opening | one paragraph: what `do-harness` is, and what this version ships |
| `## Breaking changes` | the migration steps, or an explicit `None.` — never silence |
| `## Install` | every channel, pinned to the tag |
| `## Verify the download` | `checksums.txt`, plus what each registry attests |
| `## Requirements` | platforms, MSRV, Node version |
| `## Known limitations` | what this release does not do yet |
| `## Changes` | the merged pull requests, grouped by conventional-commit type |

The header above `## Changes` comes from one of two files, both of them reviewed
in this repository:

- `docs/releases/<tag>.md` — the curated header for a single release. Use it
  whenever the release has a highlight, a known blocker, or a migration step; it
  must name the version, which the check enforces.
- `.github/release-notes-template.md` — the fallback, with `{{TAG}}` and
  `{{VERSION}}` substituted.

`release-notes.sh compose <tag> <generated> <out>` writes the header above the
change list, and `release-notes.sh check <tag> <out>` fails the release when a
required section is missing or empty, a placeholder survived substitution, the
curated header does not name the version, or the composer lost a pull request.
That last one matters: a silently dropped change is worse than a badly
categorized one. A release can therefore not ship a bare commit dump — the shape
`v0.1.1` shipped.

The change list is GitHub's generated changelog (the merged pull requests since
the previous tag). `release-notes.sh` regroups its bullets by the
conventional-commit type in each pull-request title — `feat` → Features, `fix` →
Fixes, `perf` → Performance, `docs` → Documentation, `refactor` → Refactors,
`test` → Tests, `chore`/`ci`/`build`/`style` → Maintenance, a `!` subject →
Breaking changes, anything else → Other changes — instead of by label. No pull
request in this repository carries a label (0 of 74 merged between `v0.1.1` and
`v0.1.2`), so the label categories in `.github/release.yml` put every entry in one
catch-all bucket. Commit subjects are already enforced by the commitlint sensor,
which is why they are the grouping that exists.

Preview the body without publishing:

```bash
gh api --method POST repos/d-o-hub/do-harness/releases/generate-notes \
  -f tag_name=v0.1.3 -f target_commitish=main \
  -f configuration_file_path=.github/release.yml --jq .body > generated-notes.md
bash scripts/release-notes.sh compose v0.1.3 generated-notes.md release-notes.md
bash scripts/release-notes.sh check v0.1.3 release-notes.md
bash scripts/release-notes.sh --self-test
```

The generated side needs `gh api` access; the composer and the check are offline
and hermetic, so they run in CI as the `release-notes` sensor.

The job calls `gh api …/releases/generate-notes` with `previous_tag_name` (from
`git describe` on the tag's parent) and `configuration_file_path`, then publishes
with `gh release create --notes-file`. Two behaviours are measured, not assumed:

- **For a tag that already exists, the configuration resolves from the tagged
  commit.** Generating notes for `v0.1.2` (tagged before `.github/release.yml`
  landed) ignored the file, and passing `configuration_file_path` explicitly
  answered `Could not find a configuration file at .github/release.yml`; the same
  call for a tag that does not exist yet resolves it from the default branch. The
  config therefore has to be on the commit you tag, and a release that predates
  it cannot be retrofitted — `gh release edit` takes literal notes, not generated
  ones.
- `exclude` still filters by label, which is the one input this repository does
  not maintain on pull requests; the published grouping comes from commit
  subjects instead (above), so nothing depends on it.

Anything release-specific (a highlight, a known blocker, a migration step)
belongs in the curated header — `docs/releases/<tag>.md` for one release — or
goes in afterwards with `gh release edit <tag> --notes-file <file>`.

## crates.io publishing

The publish job runs on a tag push, and on `workflow_dispatch` with
`publish=true` — the bootstrap and re-run path for a version whose tag already
exists. It is idempotent: a version already present on crates.io is skipped, so
a partially failed run can be re-run safely. Publish order is `do-harness-types` → `do-harness-db` → `do-harness`;
each step retries while the registry index catches up.

Before introducing or publishing a new publishable crate, run the pre-publish name check described in [.agents/skills/crates-io-name-check/SKILL.md](../.agents/skills/crates-io-name-check/SKILL.md). Paste the terminal output (`curl` API status and `cargo search` results) into the release PR or pre-publish record to confirm availability and naming appropriateness before the first publish.

The CLI crate embeds repository files through symlinked assets
(`crates/do-harness/assets/compliance.md` → `docs/compliance.md`,
`crates/do-harness/assets/methods.json` → `plans/methods.json`). `cargo
package` dereferences symlinks, so the published crate contains regular files
and the canonical sources never drift. The greenfield crate scaffold is stored
as `templates/crate/Cargo.toml.template` because cargo excludes nested
packages (any subdirectory with a `Cargo.toml`) from a package.

Validate packaging locally before tagging:

```bash
cargo publish --dry-run -p do-harness-types
# db and do-harness depend on the crates above being on crates.io; once
# published, their dry runs work too.
```

**Every embedded file must live inside the crate.** `cargo package` refuses
paths outside the package root, so `include_str!("../../../.config/nextest.toml")`
compiles and passes tests locally while the published tarball cannot build:
`cargo publish` fails with `error: couldn't read …: No such file or directory`
during tarball verification. Measured on the v0.1.2 tag, where the CLI crate
failed eight attempts after two others had published. External files are
symlinked into `crates/do-harness/assets/` (`compliance.md`, `methods.json`,
`nextest.toml`), which `cargo package` dereferences;
`crates/do-harness/tests/embedded_assets.rs` fails when any include path
resolves outside the crate.

**The tag preflight installs every tool its sensor set runs.** The `verification`
set's sensors need `cargo-deny` (deps), `cargo-audit` (audit), `cargo-nextest`
(test), and `shellcheck`, and the preflight lists them explicitly; adding a
sensor that needs a new tool means adding it there too. Measured on the v0.1.2
tag: a preflight without `cargo-nextest` failed `verify --strict` with
`error: no such command: nextest` and skipped the whole release.

## npm publishing

`integrations/npm/` holds the meta package (`do-harness`, a zero-policy
launcher) and five platform package templates. The npm-publish job downloads
the build artifacts, stages each platform package with its binary, and
publishes platform-first so the meta package's pinned `optionalDependencies`
resolve. Versions already on npm are skipped, so a partial run can be re-run.

The publisher's `UNAVAILABLE_PKGS` list names platform packages the registry
refuses. They are skipped with an explicit log line, and the meta package
publishes regardless — an unavailable pin is **inert**: npm filters an
`optionalDependency` by its `os`/`cpu` before fetching it, so a Linux/macOS
install never requests the Windows package (verified: clean install, clean
`npm ls`), and on Windows the missing package is skipped quietly while the shim
exits with release-zip guidance. Withholding the meta package instead made
`npx do-harness` return 404 for every Linux/macOS user, breaking the primary
install path documented in `README.md`. Before the list existed the loop
aborted on the 403, so every tag push ended with a red `npm-publish` job that
looked like a release failure while actually being the documented decision.
Keep `UNAVAILABLE_PKGS` in step with `UNAVAILABLE_PACKAGES` in
`integrations/npm/lib/platform.js`; the skill's `check-npm-sequence.sh --root .`
fails if the two disagree or if the meta publish is gated on the list again.

Committed `integrations/npm/**/package.json` versions are placeholders for
local tooling: `scripts/publish-npm.sh` patches the meta version and all five
`optionalDependencies` pins to the workspace version in the staging directory,
so a stale committed value can never be published.

The publisher probes each exact version in live and `--dry-run` modes, so a
partial rerun validates only the unpublished packages and never re-attempts an
immutable version.

If the registry rejects a new platform package with HTTP 403
`Package name triggered spam detection`, treat it as a registry policy block,
not a release bug:

1. Leave the blocked platform package unpublished; the meta package still
   publishes, and its pin on the blocked name stays inert on other platforms.
2. Ask npm Support (<https://www.npmjs.com/support>, `support@npmjs.com`) to
   review the exact name, including its purpose and repository.
3. After clearance, rerun `scripts/publish-npm.sh`; it skips live siblings and
   publishes the platform package before the meta package. Remove the name from
   `UNAVAILABLE_PKGS` and `UNAVAILABLE_PACKAGES` at the same time.
4. Rename only if Support cannot clear the name, and only as a coordinated
   change across `integrations/npm/lib/platform.js`, the platform manifest, the
   meta `optionalDependencies`, the publisher target table, tests, and docs.
   Prefer the `@d-o-hub` scope or a sufficiently distinct name; never unpublish
   the live siblings.

`npm publish --dry-run` does not perform the registry's name-similarity check,
so a passing dry run is not proof that a new name will be accepted
([npm/cli#9188](https://github.com/npm/cli/issues/9188)).

The bootstrap -> configure -> OIDC order is executable, not prose:
`.agents/skills/npm-github-publish/scripts/check-npm-sequence.sh --root .`
validates the publisher order, the meta `optionalDependencies`, the platform
map, and this runbook's step order. Run `--self-test` to confirm each check can
still fail.

Validate the assembly locally without a token:

```bash
dist=$(mktemp -d)
for target in x86_64-unknown-linux-musl aarch64-unknown-linux-musl \
    x86_64-apple-darwin aarch64-apple-darwin; do
  name="do-harness-v0.2.0-${target}"
  mkdir -p "$dist/pkg/$name"
  cp target/release/do-harness "$dist/pkg/$name/do-harness"
  tar -czf "$dist/${name}.tar.gz" -C "$dist/pkg" "$name"
done
name="do-harness-v0.2.0-x86_64-pc-windows-msvc"
mkdir -p "$dist/pkg/$name"
cp target/release/do-harness "$dist/pkg/$name/do-harness.exe"
(cd "$dist/pkg" && zip -qr "$dist/${name}.zip" "$name")
bash scripts/publish-npm.sh --dist "$dist" --dry-run
```

The wrapper shim resolves the platform package relative to itself and execs
the binary with inherited stdio; `node --test integrations/npm/test/*.test.mjs`
covers resolution, arg passthrough, exit-code propagation, and the
missing/unsupported-platform diagnostics.

## Install channels

| Channel | Command | Windows |
|---------|---------|---------|
| npx (zero install) | `npx do-harness init` | no (package unavailable) |
| npm dev dependency | `npm install -D do-harness && npx do-harness verify` | no |
| Prebuilt installer | `curl -fsSL .../scripts/install.sh \| sh -s -- --version v0.2.0` | yes (Git Bash) |
| Release zip | unzip `do-harness-v<version>-x86_64-pc-windows-msvc.zip` | yes |
| cargo-binstall | `cargo binstall do-harness` | yes |
| crates.io source build | `cargo install do-harness --version 0.2.0` | yes |
| Vendored source | `cargo install --path vendor/do-harness/crates/do-harness` | yes |

Windows has no npm channel: the `do-harness-win32-x64` platform package is
blocked at the registry (HTTP 403 name screening), so both npm rows above are
Linux/macOS-only. Use the installer, the release zip, or a cargo channel.

For git hooks with an npm install, point `DO_HARNESS_BIN` at
`node_modules/.bin/do-harness` (the shim forwards arguments to the platform
binary) or install the CLI onto `PATH` with the shell installer.

## Rollback

GitHub release assets can be deleted and the tag re-pointed if a build fails
before publication. crates.io versions are immutable: a bad version must be
yanked (`cargo yank --version <v>`) and superseded by a new patch release.
npm versions can be unpublished within 72 hours of publish, or deprecated
(`npm deprecate do-harness@<v> "reason"`) and superseded.
