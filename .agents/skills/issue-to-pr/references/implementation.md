# Per-item build loop

One queue item at a time, one branch, one PR. The repository's own workflow
guide is the authority on how to build; this page is the item-level checklist.

## 1. Classify the item

- **bug** — reproduce first; the fix lands with the reproduction as evidence.
- **feature** — reuse the repository's slice pattern for the domain layer.
- **docs** — check every claim against the code before writing it down.
- **refactor / chore** — behavior must not change; say how you know.

A compound item gets planned before code: break it into subtasks and track
them in the state database (`do-harness task add`) when the item is bigger
than one sitting; the repository task board is the durable plan.

## 2. Implement minimally

- The acceptance criteria in the issue are the contract; satisfy them and
  stop. Do not refactor neighbors, upgrade dependencies, or add
  configurability the item does not ask for.
- Respect the repository ceilings (file size cap, dependency direction,
  naming conventions); a violation is a sensor failure waiting to happen.
- New dependencies, public interface changes, and migrations are escalation
  triggers, not judgment calls.

## 3. Verify

- Iterate with targeted sensors (`do-harness verify --only check --only
  test`), then run the full suite before opening the PR. Record beats with
  `do-harness verify --record` when the repository workflow expects them.
- A sensor that skips for a missing tool is not a pass; note it explicitly.
- Fail-fast: after three consecutive failures on the same subtask, stop,
  record the error signature, and report — do not keep guessing.

## 4. Commit

- Conventional commits, lowercase subject, one logical step per commit.
- The final commit should leave the tree clean; generators and scratch files
  never enter a commit.

## 5. Done means

- The item's acceptance criteria are satisfied and covered by the tests or
  fixtures that fail without the change.
- The full sensor suite passes (or the only skips are justified and named).
- Docs and comments the change made stale are updated in the same branch.
- No scaffolding, debug output, or dead code from the session remains.
