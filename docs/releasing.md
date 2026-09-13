# Releasing

Releases are cut from `main` by pushing a `v<version>` tag. The tag must equal
`[workspace.package].version` in `Cargo.toml`; the release preflight fails
otherwise. Nothing publishes unless the tagged commit passes the full
verification set and every build target dogfoods green.

## One-time setup

- Add the `CARGO_REGISTRY_TOKEN` repository secret: create an API token at
  <https://crates.io/settings/tokens> (scope: publish new crates and versions)
  and store it under **Settings → Secrets and variables → Actions**. The
  publish job fails loudly when the secret is missing.
- Add the `NPM_TOKEN` repository secret: create a granular access token with
  publish rights at
  <https://www.npmjs.com/settings/~your-user~/tokens> and store it the same
  way. The npm-publish job fails loudly when it is missing.
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
     tarballs, each dogfooded with `init && verify`.
   - `release` — publishes the tarballs plus `checksums.txt` via
     `gh release create --verify-tag`.
   - `publish` — publishes the three crates to crates.io in dependency order.
   - `npm-publish` — publishes the four platform packages, then the
     `do-harness` meta package, to npm.

## crates.io publishing

The publish job runs only on tag pushes and is idempotent: a version already
present on crates.io is skipped, so a partially failed run can be re-run
safely. Publish order is `do-harness-types` → `do-harness-db` → `do-harness`;
each step retries while the registry index catches up.

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

## npm publishing

`integrations/npm/` holds the meta package (`do-harness`, a zero-policy
launcher) and four platform package templates. The npm-publish job downloads
the build artifacts, stages each platform package with its binary, and
publishes platform-first so the meta package's pinned `optionalDependencies`
resolve. Versions already on npm are skipped, so a partial run can be re-run.

Committed `integrations/npm/**/package.json` versions are placeholders for
local tooling: `scripts/publish-npm.sh` patches the meta version and all four
`optionalDependencies` pins to the workspace version in the staging directory,
so a stale committed value can never be published.

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
bash scripts/publish-npm.sh --dist "$dist" --dry-run
```

The wrapper shim resolves the platform package relative to itself and execs
the binary with inherited stdio; `node --test integrations/npm/test/*.test.mjs`
covers resolution, arg passthrough, exit-code propagation, and the
missing/unsupported-platform diagnostics.

## Install channels

| Channel | Command |
|---------|---------|
| npx (zero install) | `npx do-harness init` |
| npm dev dependency | `npm install -D do-harness && npx do-harness verify` |
| Prebuilt installer | `curl -fsSL .../scripts/install.sh \| sh -s -- --version v0.2.0` |
| cargo-binstall | `cargo binstall do-harness` |
| crates.io source build | `cargo install do-harness --version 0.2.0` |
| Vendored source | `cargo install --path vendor/do-harness/crates/do-harness` |

For git hooks with an npm install, point `DO_HARNESS_BIN` at
`node_modules/.bin/do-harness` (the shim forwards arguments to the platform
binary) or install the CLI onto `PATH` with the shell installer.

## Rollback

GitHub release assets can be deleted and the tag re-pointed if a build fails
before publication. crates.io versions are immutable: a bad version must be
yanked (`cargo yank --version <v>`) and superseded by a new patch release.
npm versions can be unpublished within 72 hours of publish, or deprecated
(`npm deprecate do-harness@<v> "reason"`) and superseded.
