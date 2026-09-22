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
## Guides
See references/heuristics.md for distilled heuristics.

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
5. **Never inject a token into the publish job.** The npm CLI prefers OIDC when
   both are present, but a token is a *fallback that only triggers when OIDC
   fails* — so a token silently masks a broken trust relationship with a
   different, weaker credential and can turn a clean OIDC failure into an
   `EOTP` error that looks like a 2FA problem. A job that also holds
   `id-token: write` must carry no publish token at all.
6. Set `package-manager-cache: false` on `actions/setup-node` in the publish
   job. npm's own trusted-publishing example marks caching "never use caching in
   release builds": a poisoned npm cache is executable input to a privileged
   publish, and the job holds `id-token: write`. The check below reads
   `.github/workflows/release.yml` and rejects a publish job that fails either
   rule.
7. A package name that has never been published cannot be configured for
   trusted publishing — npm exposes the Trusted Publisher page only for a
   package that already exists. Bootstrap a brand-new package with one
   authenticated `npm publish` (interactive, 2FA-approved), then configure its
   trusted publisher; OIDC covers every later release.
8. Never `npm unpublish` a published version to "fix" a mistake. Versions are
   immutable in practice: unpublishing breaks every lockfile that resolved it
   and is restricted to a short window. Publish a new patch version instead.

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
      package-manager-cache: false  # never cache in a privileged release job
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
   `support@example.com`. Ask for a review/clearance of the exact name; include
   the package's purpose, repository URL, and why it is not a typosquat.
3. Leave the blocked platform package unpublished. The meta package still
   publishes: its pin on the blocked name is inert where another platform
   package resolves, and the shim carries the release guidance on Windows.
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
four Linux/macOS platform packages are live at `0.1.1` and
`do-harness-win32-x64@0.1.1` is refused by npm's name screening. The meta
package `do-harness` is **not yet published** — it needs a one-time
authenticated bootstrap before a Trusted Publisher can be configured for it, so
CI's OIDC publish fails with `404 Not Found - PUT` until that bootstrap happens
(verified: an OIDC run against the unpublished name returns 404, not an auth
error). Once bootstrapped, `npx do-harness` resolves on Linux/macOS. Windows
installs from the release zip (or the installer, or a cargo channel); the shim
exits 1 there with that guidance.

Encode the decision where the publish loop can see it, rather than letting the
403 abort the run. `scripts/publish-npm.sh` keeps an `UNAVAILABLE_PKGS` list:
those targets are skipped with an explicit log line and the meta package
publishes regardless. Without the list a tag push reports a red publish job
that looks like a release failure while actually being the intended decision;
gating the meta publish on the list instead made `npx do-harness` 404 for every
Linux/macOS user. Keep the list in step with `UNAVAILABLE_PACKAGES` in
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

- **A trusted publisher does not imply permission to publish directly.** npm's
  docs: configurations created after **2026-09-03** allow `npm stage publish` and
  require "Allowed actions" to also permit `npm publish`; earlier ones keep their
  old behaviour. A CI `npm publish` then fails
  `403 Forbidden - PUT https://registry.npmjs.org/<pkg> - OIDC permission denied
  for this action` — the wording points at credentials, the cause is the action
  not being granted. Observed on the v0.1.2 tag for five packages that had been
  bootstrapped and trusted eleven days earlier; the fix is
  `npm trust github <pkg> --file <wf.yml> --repo <owner/repo> --allow-publish`
  (or the package's Trusted publishing → Allowed actions), one interactive 2FA
  challenge per package name.
- **Interactive 2FA approval is per-package and non-reusable.** Every
  bootstrap `npm publish` can emit its own browser approval URL; one approval
  does not authorize the next package. Never assume a prior approval covers the
  remaining packages, and never script a publish sequence around a single
  approval.
- **`actions/setup-node` exports a fake credential that silently defeats
  OIDC.** With `registry-url` set it always runs
  `core.exportVariable('NODE_AUTH_TOKEN', process.env.NODE_AUTH_TOKEN || 'XXXXX-XXXXX-XXXXX-XXXXX')`
  and writes `_authToken=${NODE_AUTH_TOKEN}` into a temp `.npmrc`. A publisher
  that branches on `NODE_AUTH_TOKEN` being non-empty therefore sees the
  placeholder, takes the token path, and never reaches the OIDC exchange —
  while also sending the literal dummy to the registry as Bearer auth
  (`401 Unauthorized`). Treat that exact value as unset *and* clear it so the
  temp `.npmrc` interpolates empty. A job holding `id-token: write` that also
  passes a token is the defect, not the fix.
- **A browser approval URL expires in minutes, so it is a poor fit for
  automation.** `npm publish --auth-type=web` prints an `npmjs.com/auth/cli/...`
  URL and waits, but the window is short: an agent-driven or hands-off run will
  usually see the process exit before a human approves it. When a scripted
  bootstrap is needed on a 2FA account, pass a one-time password explicitly
  (`--otp <code>`) or use a token scoped so no OTP is required. Reserve the
  browser flow for a human at a terminal, and never treat its expiry as a
  broken credential.
- **`npm trust` requires an interactive 2FA challenge.** Configuring a Trusted
  Publisher is a governance write: granular access tokens with
  `bypass_2fa: true` are rejected `403`, and even a normal 2FA token must
  satisfy the challenge. Prefer the native command
  (`npm trust github <pkg> --file <wf.yml> --repo <owner/repo> --allow-publish`,
  then `npm trust list <pkg>`) over hand-rolling
  `POST /-/package/{package}/trust`, and never assume a fresh bootstrap left
  trust configured — verify it.
- **A `404 Not Found - PUT` on publish means "no trust relationship", not
  "bad credentials".** When the package name does not exist yet, npm cannot
  have a Trusted Publisher for it and answers the PUT with 404. This is the
  expected state for a package that still needs its one-time bootstrap. An
  authentication problem looks different: `E401`/`ENEEDAUTH` for a missing or
  invalid token, or `EOTP` when 2FA is required and unfulfilled. On an
  `auth-only` account a token publish of a *brand-new* name still raises
  `EOTP`, so do not read that as a broken token.
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
- Prefer **staged publishing** where a human approval gate is wanted: `npm
  stage publish` accepts `npm stage publish` permission independently of
  `npm publish`, so a trusted publisher can be granted staging only and a
  maintainer approves with 2FA before the version goes public. This is the
  highest-assurance configuration for a release pipeline; direct publish is the
  lower-friction alternative and is what this repository uses.
- Granular access tokens that **bypass 2FA** are being narrowed: they are
  losing direct-publish capability and will only be able to read private
  packages or stage a publish pending human approval. Treat any such token as
  transitional and migrate the release path to OIDC.
- When a trusted-publisher configuration was created on or after 2026-09-03,
  allowed actions must be chosen explicitly (`npm stage publish` is always
  permitted; `npm publish` is opt-in). A configuration that suddenly refuses a
  direct publish is usually this, not a broken OIDC exchange.

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
