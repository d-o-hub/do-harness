# gh resilience

Read before `gh pr create` / `gh pr edit`, and whenever a `gh` command or a
check fails in API plumbing rather than in the diff.

## Transient failures: retry, never rethink

Route every mutating `gh` call through `scripts/retry.sh` (bounded attempts
with a fixed delay, so the agent makes one call instead of sleeping between
calls). Observed transient signatures, all safe to retry:

| Signature | Example |
|-----------|---------|
| HTTP 502/503/504 | `pull request create failed: HTTP 502: 502 Bad Gateway (https://api.github.com/graphql)` |
| GraphQL internal error | `GraphQL: Something went wrong while executing your query on ... Please include <tracking-id>` |
| Connection / timeout | `Failed to connect`, `Connection reset`, `timed out` |
| Rate limit | `HTTP 429`, `rate limit exceeded` |

```bash
scripts/retry.sh --attempts 5 --delay 15 -- gh pr create --title ... --body ...
```

`retry.sh` replays stdout only on success, returns the final exit code, and
refuses to retry deterministic failures — so a repeated failure after the
bounded attempts is evidence, not a cue to try harder.

## Deterministic failures: route around, do not retry

- **Projects-classic `projectCards` error** (`GraphQL: Projects (classic) is
  being deprecated ... (repository.pullRequest.projectCards)`): `gh`'s
  default view/edit GraphQL query is broken server-side. Never retry; use
  the REST API instead:
  ```bash
  gh api -X PATCH "repos/{owner}/{repo}/pulls/<N>" -f body="..."
  gh pr view <N> --json number,title,body,state  # explicit fields only
  gh issue view <N> --json number,title,body     # never --comments
  ```
- **Rerun forbidden** (`cannot be rerun; its workflow file may be broken`,
  HTTP 403 `Jobs in this workflow run cannot be re-run`): the run cannot be
  restarted. If the failure is infrastructure with zero diff causation (see
  below), push one empty retrigger commit and continue the sweep; if the
  retried run fails the same way, escalate instead of committing again.
  ```bash
  git commit --allow-empty -m "chore(ci): retrigger checks after transient <name> failure"
  ```
- Anything else 4xx, `already exists`, validation errors: fix the invocation.

## Duplicate-PR caution

Retrying `pr create` across a 502 is safe (the mutation never committed),
but if a retry succeeds after several attempts, confirm no duplicate was
left behind before merging:

```bash
gh pr list --head <branch> --json number,state
```

## Known-transient check failure

CodeQL `Analyze (rust)` failing at `Uploading results` with a
`codeql-failed-run.sarif` post-step while extraction reports `0 files with
errors` is SARIF-upload plumbing, not analysis. When the PR touches no Rust
files and the default branch is green, treat it as transient: empty
retrigger commit, one cycle, then escalate per check-policy.
