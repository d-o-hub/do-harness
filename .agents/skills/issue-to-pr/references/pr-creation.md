# PR creation

Read this before pushing a branch or opening a pull request.

## Branch and base

- `start-item.sh` names branches `issue-ID-slug` / `task-ID-slug` and cuts
  them from fresh `origin/main`.
- Never push the base branch, never reuse another item's branch, never
  force-push.

## Creating the PR

`open-pr.sh` pushes the branch and creates the PR through the REST API
(`gh api -X POST repos/{owner}/{repo}/pulls -f ...`). The REST path is
deliberate: the Projects-classic class of GraphQL failures does not touch it,
and the body is a genuine field rather than a template. Mutating `gh` calls
route through `scripts/retry.sh`, which retries only transient failures and
refuses deterministic ones.

The script checks for an existing PR on the branch first and reports it
instead of creating a duplicate; re-running it after a failure is safe.

## Body contract

- First line: what changed, in one sentence.
- Then: why, and the observable verification evidence (sensor names,
  fixtures, or a manual reproduction) — claims without evidence do not ship.
- Issues end with `Closes #ID` so the merge closes the item. Plan tasks have
  no tracker trailer.
- Never paste secrets, tokens, or machine-local paths into a body.

## After the PR exists

Hand off to `pr-triage` (repository PR lifecycle): it reviews, resolves
conversations, waits for checks, and merges with `--match-head-commit`. This
skill does not merge, does not disarm auto-merge, and does not touch review
threads.
