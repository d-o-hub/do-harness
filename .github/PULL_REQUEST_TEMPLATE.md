## Summary

<!-- One sentence: what changed and why. -->

## Verification

<!-- Evidence: sensor names and results, or a manual reproduction. -->

## Checklist

- [ ] Description reflects the change.
- [ ] Relevant sensors run and pass (see the commit / CI checks).
- [ ] No unrelated changes included.

### Contract-change checklist

Only required when this PR touches the adoption contract (the
`do-harness.toml` schema): `crates/do-harness/src/config/**`,
`crates/do-harness/src/migrate.rs`, `crates/do-harness/src/doctor/checks.rs`,
`crates/do-harness/templates/do-harness.toml.*`, `do-harness.toml`,
`docs/versioning.md`, or `docs/contract-migrations.md`.

- [ ] `CONFIG_SCHEMA_VERSION` and every template's `schema-version` bumped
      together (or the change is annotated `Contract-schema: additive-only`).
- [ ] A migration note is included in `docs/contract-migrations.md` and in
      this PR body (or the change is annotated `Contract-schema: additive-only`).
- [ ] Reviewed by the contract owner per `CODEOWNERS`.
