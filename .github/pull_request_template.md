## Summary

Describe the change in one sentence, then explain why it is needed.

## Issue link

Every PR must reference a triaged issue (see CONTRIBUTING.md). Use a trailer
in the PR body:

- `Closes #<n>` — this PR fully implements the issue and will close it on merge.
- `Refs #<n>` — this PR partially addresses the issue; the remaining step is
  tracked in the issue.

## Consumer impact

List any required upgrade steps. Add one line per action as
`Consumer-Impact: <group> | <action>`. Groups are `cli`, `npm`, `packs`,
`guides`, and `contract`. For example, use
`Consumer-Impact: contract | Run do-harness migrate --write`.

## Verification

List the evidence that your change passes the project sensors:

- [ ] `do-harness verify --set verification --changed` is green locally.
- [ ] Full CI (`verify.yml`) is green on the pushed branch.
- [ ] For docs changes: `python3 scripts/docs.py links --check` is green.
- [ ] Commit messages pass commitlint (Conventional Commits, lowercase subjects).

## Checklist

- [ ] Branch name follows the documented convention (`feat/`, `fix/`,
      `chore/`, `docs/`, or `issue-<n>-<slug>` / `task-<n>-<slug>`).
- [ ] No secrets, tokens, or machine-local paths are in the diff or body.
