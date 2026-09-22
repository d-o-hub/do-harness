# Merge order and update rules

## Ordering

1. **Stacks first.** A PR is stacked when its `baseRefName` is another open PR's
   `headRefName` (or is not the repository default branch). Merge the parent
   before the child.
2. **Oldest first.** Among independent PRs, ascending `createdAt`.
3. **One at a time.** Never merge two PRs in parallel. After each merge, the next
   PR is updated against the new base before it is validated.

`scripts/list-prs.sh` implements this ordering.

## Priority within a sweep

Stacks-first and oldest-first decide *when* a PR is eligible; this decides which
eligible PR to take next when several are ready:

1. trivial green (docs, chores, dependency bumps whose checks pass);
2. security and clamp fixes — anything that reduces exposure while it waits;
3. foundation before dependents (a library change before the PR consuming it);
4. one keeper per cluster
   ([duplicate-clusters.md](duplicate-clusters.md)); close the losers before
   reviewing the keeper;
5. mega-PRs, `unsafe`, and SIMD last: they need the most review budget and
   rebase the worst.

## Updating a branch

- Independent PR behind base: `gh pr update-branch PR` (creates a merge commit;
  preserves approvals; the new head must be re-validated).
- Never `git push --force` an independent PR; only stacks need the rebase below.

## Stacked PR after the parent merges (squash or rebase)

Merging the parent leaves the child's branch containing the parent's old
commits, which re-appear in the child diff. If the parent branch is
auto-deleted, GitHub retargets the child to the default branch and usually
reports `CONFLICTING` / `DIRTY` until it is rebased. Rebase the child onto the
new default branch:

```bash
git fetch origin
git switch CHILD_BRANCH
git rebase --onto origin/DEFAULT OLD_PARENT_SHA CHILD_BRANCH
git push --force-with-lease origin CHILD_BRANCH
```

`OLD_PARENT_SHA` is the parent head recorded in the state file before the parent
merge. The rebase is required after a rebase merge as well as a squash merge:
both rewrite the parent's commits, so the child's recorded head is no longer an
ancestor of the default branch. After the force-push, the child has a new head
SHA: re-run checks, conversations, and review before merging. Force-push only
own or bot branches.

If the rebase conflicts beyond trivial textual overlap, abort and escalate.

## Direct merge

```bash
gh pr merge PR --squash --match-head-commit HEAD_SHA
```

Use the repository's configured merge method (squash or rebase) and always pin
the validated head with `--match-head-commit`.

Requirements before running it:

- `mergeStateStatus` is not `BEHIND`, `DIRTY`, or `UNKNOWN`.
- Every check passes or is an allowed skip (see `check-policy.md`).
- No unresolved review threads.
- No blocking review findings.
- No auto-merge request is armed: it merges whatever head exists when GitHub
  next computes mergeability, defeating the pin. The sweep disarms any
  pre-existing request (`scripts/auto-merge.sh PR --disable`) before validating
  and halts if the disarm fails.
- The head SHA equals the one validated, and the merge command pins it.

Never pass `--auto`, and never leave an auto-merge request armed while the sweep
holds more than one PR: the request fires on whatever head exists when GitHub
computes mergeability, so it races the per-PR loop below and can land a tree
nobody validated. `scripts/auto-merge.sh PR --disable` disarms one that is
already armed; halt the sweep if it cannot be disarmed.

## Per-PR loop

Every merge invalidates the next PR's validation, so after `gh pr merge`:

1. rebase or update the next PR against the new base
   (`gh pr update-branch PR`, or the stacked rebase above);
2. re-run `scripts/checks.sh PR --wait 1200` — a green from before the rebase is
   evidence about a different tree;
3. re-read `gh pr view PR --json mergeStateStatus,mergeable` and confirm the head
   SHA you are about to pin;
4. merge with `--match-head-commit`, then repeat from step 1 for the next PR.

A sweep of one PR still follows the loop; it simply has no successor to rebase.

## Post-merge verification

1. Read the merge commit SHA from `gh pr view PR --json mergeCommit`.
2. Confirm default-branch runs for that SHA are green:
   `gh run list --commit MERGE_SHA --limit 20`.
3. On failure: create a revert PR from the default branch
   (`git revert -m 1 MERGE_SHA` on a new branch), open it, wait for checks, and
   merge it with the same direct procedure. Then halt the sweep and report.

## Branch deletion

Do not delete PR branches during triage. Deleting a parent branch retargets its
stacked child and changes the child's diff; leave deletion to the repository's
own automation or the user.
