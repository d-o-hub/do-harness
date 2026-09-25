# Release runbook

**Scope**: cutting a release from this repository.
**Preflight**: `scripts/check-release-preflight.sh` — read-only; it never
tags, pushes, or publishes.
**Sensor**: `release-preflight` runs the offline half of the preflight
(version pins) in the `verification` and `release` signal sets.

## Golden path

1. **Bump every version pin in one PR.** `VERSION` (when the repository has
   one), `Cargo.toml` (`[workspace.package].version` when the workspace
   declares one, otherwise `[package].version`) and every non-private
   `package.json` must all read the new version.

   ```bash
   bash scripts/check-release-preflight.sh   # pins agree at the target version
   do-harness verify --set release           # full pre-release gate
   ```

2. **Prove the target is unpublished.** This is the check that catches a
   release dispatched against a version that already shipped:

   ```bash
   bash scripts/check-release-preflight.sh --release
   ```

   It fails with `already has a GitHub Release` and exits 1. It needs `gh`
   with network access; when either is missing it prints `WARN` and exits 0,
   so the local loop stays usable. With `CI=true` or
   `DO_HARNESS_REQUIRE_TOOLS=1` a skipped comparison is a `FAIL` instead — a
   guard rail that cannot run is not a guard rail.

3. **Merge** the release-prep PR once CI is green.

4. **Tag or dispatch** the release workflow.

5. **Verify the release exists** rather than assuming the dispatch worked:

   ```bash
   gh release view vX.Y.Z
   gh release list --limit 3
   ```

## What blocks a bad release

| Check | Where | Behaviour |
| --- | --- | --- |
| Version pins agree | `release-preflight` sensor, and step 1 | `FAIL` on drift; offline and deterministic |
| Target not already published | step 2 (`--release`) | `FAIL` when the release exists; `WARN` without `gh`/network |
| Everything else | `do-harness verify --set release` | `FAIL` on the first red sensor |

The already-published comparison is deliberately not a sensor: the committed
version legitimately equals the last release between releases, so a sensor
that fails on it would gate every ordinary run. It belongs to the release
step, which is what step 2 is for.

## Failure playbook

| Symptom | Fix |
| --- | --- |
| `already has a GitHub Release for <version>` | Bump every pin in a release-prep PR, merge it, then re-run the preflight. Re-dispatching without a bump fails the same way. |
| `<pin> declares <x>, but the target version is <y>` | Make the pins agree, or take the file out of the pin set: a `package.json` marked `"private": true` is skipped, and `--no-package-json` drops the manifest pins when they are stamped at publish time instead. |
| `WARN: release comparison skipped` | Install `gh` and authenticate (`GH_TOKEN`), or set `DO_HARNESS_REQUIRE_TOOLS=1` where the guard must fail closed. |
| Release workflow fails on `Determine version` | The tag and the committed version disagree. Tags are immutable in practice: bump the version instead of moving the tag. |
