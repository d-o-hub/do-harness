---
name: npm-github-publish
description: >
  Publish npm packages safely from GitHub Actions using npm Trusted Publishers
  and OIDC instead of long-lived write tokens. Use for npm release workflows,
  multi-package or platform-package publishing, provenance, idempotent version
  checks, registry spam-rejection handling, coordinated package renames, or
  migrating NPM_TOKEN jobs to GitHub OIDC.
license: MIT
metadata:
  short-description: Publish npm packages with GitHub OIDC
  tags: npm github-actions oidc trusted-publishing provenance release
---

# GitHub npm publishing

Use this skill when a repository publishes npm packages from GitHub Actions or
when a token-authenticated npm release needs a safer OIDC migration.

## Trusted-publisher contract

1. Configure a trusted publisher separately for every package on npmjs.com.
   Select GitHub Actions and enter the exact organization/user, repository, and
   workflow filename, including `.yml`. For `d-o-hub/do-harness`, the workflow
   is `release.yml`. If the workflow runs `npm publish`, allow direct publish;
   stage-only configuration is incompatible with that command.
2. Give only the publishing job `id-token: write` and keep `contents: read`.
   Do not add `NPM_TOKEN`, `NODE_AUTH_TOKEN`, or a broad repository token to the
   publish job. Use a GitHub-hosted runner; npm trusted publishing does not
   support self-hosted runners.
3. Use Node >= 22.14.0 and npm >= 11.5.1. Pin or select a Node release that
   satisfies both, then check the versions before publishing so an old npm
   cannot silently fall back to token authentication.
4. Keep the package `repository.url` exactly aligned with the GitHub repository.
   npm validates this relationship for GitHub trusted publishing.

## Release workflow

Keep build and publish responsibilities separate:

1. Preflight the tag/version, run the full verification set, and build every
   target before the publish job starts.
2. In the publish job, download only the verified artifacts, configure the npm
   registry, check the Node/npm versions, and run the publisher script.
3. For platform packages, stage and publish each exact version first. Publish
   the meta package last because its `optionalDependencies` must resolve to
   versions already present on npm.
4. Make reruns idempotent. Query the public npm registry metadata with an
   unauthenticated HTTP `GET` for the exact package/version and skip an exact
   version that already exists. Do not use `npm whoami` to test OIDC: npm's
   documentation states that `whoami` does not reflect OIDC authentication,
   which is exchanged only during `npm publish` or `npm stage publish`.
5. Keep a `--dry-run` assembly path that extracts the same release artifacts
   and rewrites staged versions. Run the exact-version registry probe in both
   live and dry-run modes: skip versions already on npm, then run
   `npm publish --dry-run` for each unpublished package. This preserves
   publish-time access/registry checks; `npm pack --dry-run` may supplement
   payload inspection but is not a substitute. Verify all package manifests
   and binaries before a tag release.

Example job shape:

```yaml
permissions:
  contents: read
  id-token: write
steps:
  - uses: actions/setup-node@<pinned-sha>
    with:
      node-version: 24
      registry-url: https://registry.npmjs.org
  - run: npm --version
  - run: bash scripts/publish-npm.sh --dist dist
```

The npm CLI exchanges the GitHub Actions OIDC token during `npm publish`; no
manual token exchange or `--provenance` flag is needed. For public packages
published from GitHub Actions, npm generates provenance automatically.

## Registry-name rejection and rename decisions

The npm registry can reject a new unscoped package with HTTP 403
`Package name triggered spam detection`. This is a registry policy decision,
not an npm CLI authentication or dry-run failure. Do not blind-retry it.

Use this decision path:

1. Stop the ordered release. Record the exact package, version, npm account,
   registry URL, complete error text, and the already-published siblings.
2. Contact npm Support at <https://www.npmjs.com/support> or
   `support@npmjs.com`. Ask for a review/clearance of the exact name; include
   the package's purpose, repository URL, and why it is not a typosquat.
3. Keep the blocked platform package and the meta package withheld. The meta
   package must not reference an unavailable optional dependency.
4. After clearance, rerun the same idempotent publisher. It skips existing
   sibling versions, publishes the cleared platform package, and publishes the
   meta package last.
5. Rename only if Support declines or cannot clear the name. Prefer the npm
   user/organization scope for a replacement name, or choose a sufficiently
   distinct name; npm's guidance identifies scopes as the safe namespace for
   names that collide with similarity protections.
6. A rename is a coordinated public API migration, not a one-line manifest
   workaround. Update the platform map, platform manifest, meta
   `optionalDependencies`, publisher target table, tests, documentation, and
   trusted-publisher package list together. Publish the replacement platform
   before the meta package. Never unpublish the four live siblings to make a
   rename possible.

   Cross-reference consistency is checked, not trusted:

   ```bash
   bash .agents/skills/npm-github-publish/scripts/check-npm-sequence.sh --root .
   ```

   It fails if a platform manifest's `name` disagrees with the publisher's
   target table, if a platform package is unpinned by the meta package, or if
   the shim map no longer resolves it — so a partial rename cannot land. Its
   `--self-test` includes a manifest-only rename control that proves that
   specific failure is detected.

Renaming to a scope (npm's guidance for names similarity-blocked) carries two
extra requirements that do not apply to an unscoped swap:

- **Scoped packages default to private.** A scoped replacement needs explicit
  public access or it publishes a private package: set `publishConfig.access`
  to `public` in the staged manifest or pass `--access public`. This is a
  silent failure mode, not an error.
- **The registry probe already handles scoped names.** The publisher's
  `https://registry.npmjs.org/<pkg>/<version>` GET resolves a scoped
  `@scope/name` correctly without percent-encoding the slash (verified against
  a live scoped package: existing version 200 with the scoped `name` in the
  body, absent version 404). Do not "fix" it by encoding the slash.

Verify a replacement name is actually publishable before committing to the
migration; npm has no rename operation, so a name that is rejected or already
taken forces another migration.

For `d-o-hub/do-harness`, the decision is **Windows via GitHub release only**:
four Linux/macOS platform packages are live at `0.1.1`,
`do-harness-win32-x64@0.1.1` is refused by npm's name screening, and
`do-harness@0.1.1` is withheld. Windows installs from the release zip (or the
installer, or a cargo channel); `npx do-harness` is Linux/macOS-only.

Encode the decision where the publish loop can see it, rather than letting the
403 abort the run. `scripts/publish-npm.sh` keeps an `UNAVAILABLE_PKGS` list:
those targets are skipped with an explicit log line and the meta package is
withheld while the list is non-empty. Without it a tag push reports a red
publish job that looks like a release failure while actually being the intended
decision. Keep the list in step with `UNAVAILABLE_PACKAGES` in
`integrations/npm/lib/platform.js`; the sequence guard fails if they disagree.

A dry run is not proof that a name will pass registry similarity checks; npm CLI
issue [#9188](https://github.com/npm/cli/issues/9188) documents that limitation.

Two facts that keep this documented failure precise:

- **An unpublished optional dependency does not block publishing.** `npm
  publish` succeeds even when a pinned `optionalDependencies` entry is absent
  from the registry (verified with `--dry-run` against an absent package), and
  `npm install` of that meta package also succeeds. The failure therefore
  surfaces at *run* time, when the shim cannot resolve the binary.
- **That is why the shim, not the install step, carries the guidance.** An
  unavailable platform package should exit pointing at the release channel;
  `--include=optional` is wrong advice because nothing is disabled.
  `integrations/npm/lib/platform.js` keeps the list in `UNAVAILABLE_PACKAGES`
  and `shim.test.mjs` pins the behavior.

## Sequence checks are executable

Do not trust prose for the release order. The order and its consistency checks
live in `scripts/check-npm-sequence.sh`, which validates the publisher's
platform-before-meta order, the meta `optionalDependencies` pins, the runtime
platform map, and the runbook's step order:

```bash
bash .agents/skills/npm-github-publish/scripts/check-npm-sequence.sh --root .
bash .agents/skills/npm-github-publish/scripts/check-npm-sequence.sh --self-test
```

`--self-test` mutates a synthesized fixture to prove every check can still
fail; a check that cannot fail is decorative, so a green `--root` run alone is
not evidence. Update this script — not another paragraph — when the order
changes.

## Anti-patterns (negative knowledge)

These are recorded failures, not passing fixes. They are graded by negative
assertions (`absent:` / `not-contains:`), so they are provable even though no
positive fix ever passed:

- **Interactive 2FA approval is per-package and non-reusable.** Every
  bootstrap `npm publish` can emit its own browser approval URL; one approval
  does not authorize the next package. Never assume a prior approval covers the
  remaining packages, and never script a publish sequence around a single
  approval.
- **A package must exist before it can have a Trusted Publisher.** npm exposes
  **Settings → Trusted Publisher** only for published packages, so
  "configure all six, then publish" is an impossible instruction. Bootstrap
  each package first, then configure, then let Actions/OIDC publish subsequent
  versions.
- **`npm publish --dry-run` is not a name-availability check.** It validates
  staged payload assembly but skips the registry's similarity/spam screening,
  so a passing dry run is not proof a new name will be accepted. Probe the
  registry and keep the publisher idempotent instead.
- **Never rename one reference.** Changing only the platform manifest, only the
  meta `optionalDependencies`, or only the runtime map produces a wrapper that
  silently resolves nothing on that platform.
- **Never unpublish live siblings to make a rename possible.** Immutable
  versions cannot be reused, and the four published platform packages are
  depended on by the meta package.

References: [Trusted publishing](https://docs.npmjs.com/trusted-publishers),
[package name guidelines](https://docs.npmjs.com/package-name-guidelines/),
[threats and mitigations](https://docs.npmjs.com/threats-and-mitigations/),
[npm package-name rules](https://blog.npmjs.org/post/168978377570/new-package-moniker-rules.html),
[unpublishing](https://docs.npmjs.com/unpublishing-packages-from-the-registry/),
and npm's [registry-side spam error guidance](https://github.com/npm/npm/issues/20866).

## Security and operations

- Prefer exact versions and platform-specific package constraints (`os` and
  `cpu`) so an optional dependency cannot resolve a different release.
- Avoid using registry read commands as authentication probes. Public metadata
  reads can remain anonymous; the publish command is the OIDC boundary.
- After validating the migration, revoke unused write tokens and enable npm's
  “require two-factor authentication and disallow tokens” package setting.
- Check trusted-publisher fields case-sensitively when authentication fails:
  repository, workflow filename, provider, and optional environment must match
  the npm configuration. OIDC failures should stop the release; never fall
  back to a guessed or newly minted token.
- Trusted publishing does not authenticate private dependency installation or
  arbitrary npm commands. Use a separate read-only token only when private
  dependencies require it, never for the publish credential.

## Verification checklist

Before merging or tagging:

- The exact-version probe skips live versions, and `npm publish --dry-run`
  passes for each unpublished staged package; `npm pack --dry-run` alone is
  insufficient for publish validation.
- The workflow grants `id-token: write` only to the npm publish job and has no
  publish-token secret reference.
- Node/npm version guards cover the npm OIDC minimums.
- The exact workflow filename configured at npm matches the file in
  `.github/workflows/`.
- Shellcheck and the repository's release/verification sensors pass.
- A failed OIDC exchange fails the job; it does not skip a package or publish
  with an unverified fallback.
