# Versioning

`do-harness` is a multi-component system whose parts change at different rates
and impact different consumers. This repository versions each artifact
independently instead of forcing one version on all of them: a patch to the
Python sensor pack does not bump the CLI release, adopters can pin the
adoption contract they parse against, and a guide change that alters expected
agent behavior carries its own version signal.

## Artifacts and version sources

| Artifact | Consumer | Version source | Version file |
| :--- | :--- | :--- | :--- |
| CLI binary | end users, CI pipelines | git tag `vX.Y.Z` (existing release flow) | `Cargo.toml` `[workspace.package].version`; cut by `scripts/release-manager.sh` |
| Adoption contract | adopting repositories | `schema-version` (hard gate) + `contract_version` (pin-able semver) | `do-harness.toml` at the repository root; scaffolded copies under `crates/do-harness/templates/do-harness.toml.generic` and `.../do-harness.toml.web` |
| Sensor packs | sensor authors | per-pack `version` (TOML key) | `crates/do-harness/templates/packs/<lang>/manifest.toml` for `rust`, `generic`, `web`, `node`, `python`, `go` |
| Guides | agent operators | frontmatter `version` (YAML key) | `AGENTS.md` (root and template), `crates/do-harness/templates/USE_RESTRICTIONS.md` |

Every version except `schema-version` is a plain `MAJOR.MINOR.PATCH` semver —
the same shape the release pipeline accepts for the CLI. Shared scaffold files
that are not a versioned artifact (e.g. `templates/scripts/check-commitlint.sh`)
track the CLI and are not independently versioned.

## The adoption-contract version

The adoption contract is the `do-harness.toml` schema adopters depend on. Its
version is the `schema-version` key, kept in three places that must always
agree:

- `CONFIG_SCHEMA_VERSION` in
  `crates/do-harness/src/config/mod.rs` — the version the binary understands;
- `schema-version` in the scaffolded templates
  `crates/do-harness/templates/do-harness.toml.generic` and
  `crates/do-harness/templates/do-harness.toml.web` — the version `init`
  writes for new adopters;
- `schema-version` in the repository's own `do-harness.toml` — the version
  this repository itself declares.

The config parser (`crates/do-harness/src/config/`) consumes the contract;
`do-harness migrate` (`crates/do-harness/src/migrate.rs`) upgrades older files;
`doctor` reports a skew between the declared version and the binary's.

Semantics of a declared version:

- A config that declares a `schema-version` **greater** than the binary's is
  refused, fail-closed: the binary cannot know what a future key means.
- A config that declares the **same or an older** version parses. `migrate`
  upgrades older files; today it declares the version and replaces the
  deprecated `allow_failure` alias with `severity = "warn"`.
- An absent `schema-version` is version 0, the pre-versioning schema;
  `migrate` adds the key.

Alongside the integer gate, the contract carries the optional
`contract_version` semver key: an independent, pin-able version adopters can
reference without the CLI release. Absent means the pre-versioning contract
and parses as `0.0.0` (backward compatible); when present it must be
well-formed plain semver or config loading fails (the same shape the release
pipeline accepts). A contract change bumps `schema-version` per the rules
below **and** `contract_version` analogously (the semver bump follows the
consumer-centric rows in the bump-rule table).

## Contract bump rules

A change to the contract is any edit to a contract-definition file:

- the config parser `crates/do-harness/src/config/`;
- the migrations `crates/do-harness/src/migrate.rs`;
- the doctor's schema-version checks `crates/do-harness/src/doctor/checks.rs`;
- the scaffolded templates `crates/do-harness/templates/do-harness.toml.*`.

| Change | Version bump |
| --- | --- |
| Add a new key/field a config file may carry | required, or the explicit additive-only annotation |
| Rename or remove a key/field | required |
| Change the meaning of an existing key/field | required |
| Change requiredness or defaults | required |
| Change what `deny_unknown_fields` accepts | required |
| Any edit to a contract-definition file, additive or not | required, or the explicit additive-only annotation |

Reasoning: the parser is fail-closed on unknown keys, so even a purely
additive key changes what an old binary accepts; a new version is the only
signal an adopter can rely on. The one exception is a change that is purely
additive and explicitly annotated (below), reserved for non-breaking edits
such as documenting existing behavior or introducing an optional,
default-valued key.

## The contract-first workflow

Any PR that touches a contract-definition file must do one of the following.
The `contract-first` sensor (scripts/check-contract-first.sh) enforces both
branches; it runs in the `verification`, `main`, and `release` signal sets,
and in the pre-push hook.

1. **Bump the version.** Raise `CONFIG_SCHEMA_VERSION` and the `schema-version`
   key in every template together (the sensor fails if they disagree). Add a
   migration note — a short record of what changed, why, and how adopters
   move — to [contract-migrations.md](contract-migrations.md), and include the
   same note in the PR body. The change is reviewed by the contract owner in
   `CODEOWNERS`.
2. **Annotate an additive-only change.** If the change is purely additive and
   deliberately does not bump the version, mark it with the commit trailer
   `Contract-schema: additive-only`. This is an explicit, reviewable signal
   that the omission is intentional; a change without either the bump or the
   trailer fails the sensor.

The repository's own `do-harness.toml` is an instance of the contract, not a
definition: editing it (for example, adding a sensor) does not require a bump.
The sensor still checks that its `schema-version` agrees with the constant.

## Bump rules for packs and guides

A version bump is required whenever the artifact's shipped files change. The
rule table below is consumer-centric: it says when a bump is *major*, *minor*,
or *patch* for the people who consume that artifact.

| Artifact | Major | Minor | Patch |
| :--- | :--- | :--- | :--- |
| Sensor packs | a pack's emitted sensor commands or output contracts change incompatibly | a sensor is added to the pack | fixes that change no sensor behavior |
| Guides | agent-facing behavior changes incompatibly | a new guidance section or requirement is added | editorial fixes |

Pre-1.0, the minor is the major bump for the contract, packs, and guides, the
same convention the release derivation applies to the CLI.

## Enforcement

Two sensors enforce the version gates:

- `contract-first` (scripts/check-contract-first.sh) gates the adoption
  contract per the workflow above.
- `artifact-versions` (scripts/check-artifact-versions.sh) fails when a pack
  or guide's shipped files changed since the merge base without a strict
  increase in that artifact's version field. Its base resolution is
  `$CHECK_ARTIFACT_VERSIONS_BASE` if set, else `origin/main`, else `main`, else
  `HEAD~1`; it diffs the merge base of HEAD and that ref, sees committed
  changes since the merge base *and* uncommitted working-tree changes to the
  artifact's owner paths, and fails closed (exit 2) on unreadable git state or
  a missing/malformed version file — it never passes vacuously. `--self-test`
  proves both directions hermetically.

Satisfy the gate by bumping the artifact's version field in the same change
that touches its shipped files. The version fields are also read at release
time: the GitHub Release body's `## Artifacts` table lists every artifact
version (see [releasing.md](releasing.md), "Release notes").

## Relationship to the single-version flow

The existing single-version consumers are unaffected. `scripts/release-manager.sh`
still derives and cuts the CLI tag exactly as before; `contract_version` is
optional and absent files parse as before; the release workflow still requires
the tag to match `[workspace.package].version`. The release notes now *also*
carry the independent artifact versions, so a release body shows exactly which
contract, pack, and guide versions ship with a given CLI.

See the [Consumer Impact Matrix](releasing.md#consumer-impact-matrix) for how
release notes report changes and required upgrade steps.
