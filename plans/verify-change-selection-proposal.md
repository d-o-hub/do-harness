# Proposal: make `verify` change-selection usable outside the working tree

Status: **proposed** · Measured on 2026-11-09 · Affects `do-harness` CLI, CI policy

## Why this is a CLI problem

The measurement that started this was a CI question: a documentation-only pull
request waited 14.8 minutes for a gate that could not possibly fail. The
prototype gating that fixed it reused `when-changed` — a surface the CLI already
ships — and that reuse is what exposed the defects below. They are not
workflow papering: each one is a wrong answer or a refusal from the tool, on
the exact invocation the tool's own scaffolded guidance tells an adopter to run.

Two runs, same prototype, one variable (did the diff touch Rust):

| Plan | Wall clock | Jobs | Runner-seconds |
| --- | --- | --- | --- |
| `full` (Rust file in the diff) | 14.8 min | 11 | 4,298 |
| `scoped` (docs only) | 3.8 min | 2 | 217 |

Scoping skipped 3,751 of the 4,079 runner-seconds saved: `windows-tests`
shards 711+868+873 s, `windows` 322 s, `source-distribution` 240+377 s, `msrv`
313 s, `selection-portability` 47 s. The scoped run was still green under
`--strict` and `pr-required` passed, so the narrower gate was a real gate.

That is the prize. The defects are the cost of reaching it.

## Defect 1 — `--changed` reports success when nothing was checked

`when-changed` selection needs a working-tree diff. A CI checkout of a pull
request is clean, so the diff is empty and selection falls to the sensors with
no constraint at all:

<!-- example: id=proposal_changed_clean_tree classification=historical reason="Recorded output on 2026-11-09 from a clean checkout of ed0ab32; the defect is the point, so it is not a reproducible pass" -->
```
$ do-harness verify --changed --set verification --strict   # clean tree
PASS  commitlint
All sensors passed.
EXIT=0
```

`commitlint` is the only sensor in `verification` without a `when-changed`
constraint, so 35 of 36 sensors did not run and the command still exited `0`.

This is not "vacuous" as the repository defines it. `vacuous: true` fires only
when **zero** sensors are selected (`crates/do-harness/src/report.rs`,
`VACUOUS: no sensors selected`). Selecting an incidental always-on sensor is
indistinguishable from a full pass, which is precisely the shape of the
`latest-main.sh` and SC2154 incidents this repository has already recorded: a
gate that runs and proves nothing.

Worse, the guidance ships it. `crates/do-harness/templates/AGENTS.md` tells
every adopter to run:

<!-- example: id=proposal_scaffolded_recipe classification=parser-check-only reason="Quoted from the shipped template; executed as a doc example it would assert the very defect this proposal repairs" -->
```
do-harness verify --set verification --changed --strict
```

before claiming completion. In a fresh clone that command is a green light
after one 22-millisecond sensor. AGENTS.md §4 forbids exactly this — "a green
sensor that executed zero checks is a failed gate" — but the tool does not
enforce what the guide asserts.

**Fix.** A change-aware run whose change input is empty must not be a pass.
Either fail closed (`exit 2`, "no change input: pass --since in CI or commit
the change") or report `vacuous: true`. Failing closed is the safer default and
matches the existing `--root`/policy precedent: an unusable invocation is
exit 2, not exit 1, because no sensor made a judgement.

## Defect 2 — `--root .` is refused when combined with `--set`

<!-- example: id=proposal_root_relative_refused classification=historical reason="Recorded output on 2026-11-09; documents the refusal rather than a passing invocation" -->
```
$ do-harness --root . verify --set verification --only loc
error: managed paths must be relative without traversal components   (exit 2)

$ do-harness --root /workspaces/do-harness verify --set verification --only loc
PASS  loc   (exit 0)
```

`.`, `./` and therefore `--root "$PWD"`-relative spellings fail; only an
absolute path works. Plain `--root . verify --only loc` is fine, so the trigger
is `--set` (evidence writing) plus a non-absolute root.

Root cause is narrow: `crates/do-harness/src/managed_path.rs::within` strips the
root only when the path `is_absolute()`, then rejects any `.` component. An
evidence path built from a relative root is relative and still carries `CurDir`.

Severity is real rather than cosmetic because `--root .` is the natural CI
spelling, `--root` is documented as the way to select a workspace explicitly,
and the failure is a hard exit 2 in the middle of a gate. `verify --root .`
worked in an earlier session and now does not, so this is a regression risk in a
path nobody exercises, since the repository's own workflows use `./target/...`
from the root and never pass `--root`.

## Defect 3 — `status` cannot be told the range it should judge

`status` has no `--since`. It derives the range from the **recorded evidence**
(`selection::recorded_since`), which is right for judging a prior run but leaves
no way to ask "is the evidence current for this change?" from a clean checkout.

Consequence for CI: a scoped run records range evidence, and `status` does judge
it correctly afterwards. But the reverse ordering — verify on `main`, then check
`status` against a pull request range — cannot be expressed at all. The
prototype worked only because the scoped run wrote first.

This is the least urgent of the three and may resolve itself once Defect 1
forces an explicit range everywhere.

## Proposal

**P1 — fail closed on empty change input (Defect 1).** Small, contained change
in the selection/exit path plus the `vacuous` rule. Requires a can-fail test
(`verify --changed` in a clean clone exits non-zero) added to the existing
`crates/do-harness/tests/` suite. Ships with a note in the scaffolded
`AGENTS.md` that CI must pass `--since`, since the current template recipe is a
silent no-op there.

**P2 — accept relative `--root` (Defect 2).** Normalise the root before
containment, or canonicalise the evidence path against the resolved root.
Adds a test with `.`, `./`, and a relative parent path.

**P3 — extend the scaffolded CI recipe** to
`verify --set verification --since "origin/$GITHUB_BASE_REF" --strict`, so the
template stops recommending a command that cannot work on a clean checkout.

**Non-goal.** No new required check, no change to `pr-required` or the ruleset,
and no scoped gating merged by default. The measurement proved the mechanism;
adopting change-sensitive required checks is a separate policy decision with
its own risks (a docs-only PR cannot break the binary, but a
`do-harness.toml` edit *is* docs-shaped and *does* change the gate, which is why
the prototype classifier deny-lists it rather than allow-listing prose).

## Acceptance

- `verify --changed --strict` on a clean clone of a completed pull request exits
  non-zero, and a test proves it (can fail, then pass).
- `verify --root . --set verification --only loc` exits `0`, tested for `.`,
  `./` and a relative parent.
- `python3 scripts/docs.py sync --check`, `examples --check`, `links --check`
  pass, and the regenerated scaffolded recipe is executed by `docs-examples`.
- No change to `plans/pr-merge-policy.toml` or the `main` ruleset.

## Evidence

Measured runs are recorded in `plans/ci-scope-measurement.md`. The prototype
itself (`scripts/ci_scope.py` and the `scope` job) is **not** proposed for merge
here; it lives on a throwaway branch and its numbers are the justification, not
the deliverable.
