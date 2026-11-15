# GitHub Actions lockfiles: evaluation and adoption gate

Status: **evaluation only — not adopted.** No lockfile is generated, no workflow
is rewritten, and no new sensor is enabled by this record.

## Question

GitHub made workflow dependency lockfiles generally available on 2026-10-28:
`gh actions-lock` pins every direct *and transitive* action dependency for a
repository into a committed lockfile, recording the commit and the repository's
numeric identity. Their own semantics live at
<https://docs.github.com/en/actions/concepts/security/workflow-dependency-lockfiles>.

This repository already enforces immutable references offline. The question is
whether an additive lockfile gate adds proof the existing gate cannot, and
whether it can be adopted without weakening the current policy.

## Current enforcement (unchanged)

| Sensor | Proves | Boundary |
| --- | --- | --- |
| `action-pins` (`scripts/check-action-pins.sh`) | every external action and reusable workflow references a full 40-hex commit SHA; every Docker action image carries a `sha256:` digest | offline; proves the shape of a reference, not that the commit exists |
| `actionlint` | workflows are semantically valid | offline; no dependency resolution |

A full-SHA reference is immutable but says nothing about an action's own
transitive dependencies, and nothing about whether the pinned commit still
exists upstream.

## Evidence collected (2026-11-15)

Tool: `gh version 2.103.3 (2026-10-26)`, extension `actions-lock v0.2.0`.

`gh actions-lock --help` documents the modes this evaluation depends on:

- `--no-fix` — read-only: report findings, write nothing.
- `--verify` — frozen check; fails when the lockfile is incomplete or the
  runner would reject a pin; requires network and authentication.
- `--verify-local` — offline coverage check that every action ref has a
  lockfile entry; the help text states it "does not check that the entries are
  trustworthy".
- `--relock`, `--accept-moved` — re-resolve refs to their current upstream SHA.
- `--no-migrate-local-actions` — keep `uses: ./...` instead of migrating to
  the inherently-pinned `$/...` form (migration is the default on fix runs).

Lockfile path: `.github/workflows/actions.lock` (per repository; reusable
workflow dependencies belong to the called repository's lockfile).

Read-only probes against this checkout (7 workflows):

<!-- example: id=actions_lockfile_readonly_probes classification=illustrative reason="Recorded read-only probe output from the 2026-11-15 evaluation; not a runnable contract" -->
```bash
gh actions-lock --verify-local
# 7 of 7 workflows failed: 31 not-pinned
# 12 actions are pinned to a bare SHA without a tag ref

gh actions-lock --no-fix --json=valid,findings
# exit 0; 31 findings of category "not-pinned" (severity error)
#          12 findings of category "sha-as-ref" (severity warning)
```

So: the repository holds **zero lockfile coverage today** (31 refs unpinned),
every ref resolves against the API, and the extension already classifies 12 of
this repository's deliberately SHA-pinned references as a *warning*.

## What adoption would take

1. **A fix run that writes the lockfile.** `gh actions-lock` creates
   `.github/workflows/actions.lock`, and on the same pass rewrites same-repo
   `uses: ./...` references to `$/...` unless `--no-migrate-local-actions` is
   passed. That is a workflow migration, not a verification change.
2. **A frozen check that can fail in CI.** Only `--verify` is a provenance
   check, and it needs network access plus a token (`GH_TOKEN` takes precedence
   over `GITHUB_TOKEN`). It cannot be run from the offline sensor pack.
3. **A policy decision on reference style.** The extension's preferred model is
   a symbolic tag in the workflow narrowed by the lockfile, which is why it
   warns `sha-as-ref` on bare SHAs. The `action-pins` sensor requires the
   opposite. Adopting both without a decision produces two gates that
   contradict each other on the same lines.
4. **A moved-pin policy.** `--relock` and `--accept-moved` decide when a
   dependency advance is legitimate; that belongs to the review process, not to
   this evaluation.

## Verdict

**Defer.** The additive gate cannot be proven green before the lockfile exists,
and creating it is exactly the migration this scope excludes. The guarantees
the lockfile would add are real — transitive pinning and repository identity —
but they are not reachable without the three decisions above, and none of them
is a verification-only change.

Nothing in this record changes the `action-pins` sensor, the workflows, or any
repository or cloud setting.

## Adoption path when authorized

Each step is separately reviewable and independently revertible:

1. Decide the reference style: keep bare SHAs as the workflow requirement and
   treat the lockfile as documentation plus transitive coverage, or move to
   narrowed tags with the lockfile supplying the pin. Record the decision
   before any workflow edit.
2. Run the fix pass once, on a dedicated branch, with
   `--no-migrate-local-actions` if `uses: ./...` references must stay; commit
   the resulting `.github/workflows/actions.lock` alone, so the lockfile's
   contents are reviewable without workflow noise.
3. Add the frozen gate (`gh actions-lock --verify`) to the pull-request
   workflow, not to the offline sensor pack, and document the token it needs.
   Keep `action-pins`, `actionlint`, and the Docker-digest rule in place: the
   lockfile is additive evidence, never a replacement.

## Non-goals

- Hand-parsing `.github/workflows/actions.lock` in a sensor.
- Treating `--verify-local` as provenance verification.
- Changing repository retention, cloud settings, or the security policy of any
  adopting repository.
