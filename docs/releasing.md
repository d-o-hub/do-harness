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

## Install channels

| Channel | Command |
|---------|---------|
| Prebuilt installer | `curl -fsSL .../scripts/install.sh \| sh -s -- --version v0.2.0` |
| cargo-binstall | `cargo binstall do-harness` |
| crates.io source build | `cargo install do-harness --version 0.2.0` |
| Vendored source | `cargo install --path vendor/do-harness/crates/do-harness` |

## Rollback

GitHub release assets can be deleted and the tag re-pointed if a build fails
before publication. crates.io versions are immutable: a bad version must be
yanked (`cargo yank --version <v>`) and superseded by a new patch release.
