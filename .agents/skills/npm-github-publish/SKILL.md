---
name: npm-github-publish
description: >
  Publish npm packages safely from GitHub Actions using npm Trusted Publishers
  and OIDC instead of long-lived write tokens. Use for npm release workflows,
  multi-package or platform-package publishing, provenance, idempotent version
  checks, release dry runs, or migrating NPM_TOKEN jobs to GitHub OIDC.
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
5. Keep a `--dry-run` assembly path that extracts the same release artifacts,
   rewrites staged versions, and runs `npm publish --dry-run` without any
   credential. Verify all package manifests and binaries before a tag release.

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

- `npm publish --dry-run` passes for each staged package.
- The workflow grants `id-token: write` only to the npm publish job and has no
  publish-token secret reference.
- Node/npm version guards cover the npm OIDC minimums.
- The exact workflow filename configured at npm matches the file in
  `.github/workflows/`.
- Shellcheck and the repository's release/verification sensors pass.
- A failed OIDC exchange fails the job; it does not skip a package or publish
  with an unverified fallback.
