# Merge order and update rules

## Ordering

1. **Stacks first.** A PR is stacked when its `baseRefName` is another open PR's
   `headRefName` (or is not the repository default branch). Merge the parent
   before the child.
2. **Oldest first.** Among independent PRs, ascending `createdAt`.
3. **One at a time.** Never merge two PRs in parallel. After each merge, the next
   PR is updated against the new base before it is validated.

`scripts/list-prs.sh` implements this ordering.

## Updating a branch

- Independent PR behind base: `gh pr update-branch PR` (creates a merge commit;
  preserves approvals; the new head must be re-validated).
- Never `git push --force` an independent PR; only stacks need the rebase below.

## Stacked PR after the parent merges (squash)

Squash-merging the parent leaves the child's branch containing the parent's old
commits, which re-appear in the child diff. Rebase the child onto the new default
branch:

```bash
git fetch origin
git switch CHILD_BRANCH
git rebase --onto origin/DEFAULT OLD_PARENT_SHA CHILD_BRANCH
git push --force-with-lease origin CHILD_BRANCH
```

`OLD_PARENT_SHA` is the parent head recorded in the state file before the parent
merge. After the force-push, the child has a new head SHA: re-run checks,
conversations, and review before merging. Force-push only own or bot branches.

If the rebase conflicts beyond trivial textual overlap, abort and escalate.

## Direct merge

```bash
gh pr merge PR --squash --match-head-commit HEAD_SHA
```

Requirements before running it:

- `mergeStateStatus` is not `BEHIND`, `DIRTY`, or `UNKNOWN`.
- Every check passes or is an allowed skip (see `check-policy.md`).
- No unresolved review threads.
- No blocking review findings.
- The head SHA equals the one validated, and the merge command pins it.

Never pass `--auto`.

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
