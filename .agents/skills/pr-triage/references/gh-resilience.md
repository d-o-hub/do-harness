# gh resilience

Read before `gh pr create` / `gh pr edit` or API plumbing failures.

## Transient Failures (Retry)
Route mutating `gh` calls through `scripts/retry.sh`:
```bash
scripts/retry.sh --attempts 5 --delay 15 -- gh pr create --title ... --body ...
```
Retries HTTP 502/503/504, GraphQL errors, connection timeouts, and HTTP 429 rate limits. Refuses deterministic failures.

## Deterministic Failures (Do Not Retry)
- **Classic Projects `projectCards` error**: Use REST API with explicit fields:
  ```bash
  gh api -X PATCH "repos/{owner}/{repo}/pulls/<N>" -f body="..."
  gh pr view <N> --json number,title,body,state
  gh issue view <N> --json number,title,body
  ```
- **Rerun forbidden (HTTP 403)**: When CI failure has zero diff causation, push one empty retrigger commit; escalate if it fails again:
  ```bash
  git commit --allow-empty -m "chore(ci): retrigger checks after transient <name> failure"
  ```
- **Other 4xx / validation errors**: Fix the invocation.

## Duplicate-PR Caution
After retrying `pr create`, confirm no duplicate exists before merging:
```bash
gh pr list --head <branch> --json number,state
```

## Known-Transient Check Failure
CodeQL `Analyze (rust)` failing at SARIF upload (with `0 files with errors`) on a non-Rust PR is plumbing: push one empty retrigger commit, wait one cycle, then escalate.
