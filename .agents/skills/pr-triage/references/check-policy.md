# Check policy

## Sources and binding

- `gh pr checks PR --json name,state,bucket,workflow,link` classifies the PR's
  current checks into `pass`, `fail`, `pending`, `skipping`, or `cancel`.
- This output is not bound to a commit. Always read the current
  `headRefOid` and classify check runs for that SHA:
  `gh api repos/{owner}/{repo}/commits/HEAD_SHA/check-runs`.
- Also read legacy commit statuses:
  `gh api repos/{owner}/{repo}/commits/HEAD_SHA/status`.
- `scripts/checks.sh` performs both calls and reports `PASS`, `FAIL`,
  `PENDING`, or `SKIP` per check, plus a summary verdict.

## Verdicts

| Verdict | Meaning | Action |
|---------|---------|--------|
| `PASS` | Conclusion `success` or status `success` | Continue |
| `PENDING` | `status` not `completed`, or status `pending` | Wait and re-run; never merge |
| `FAIL` | Conclusion `failure`, `timed_out`, `action_required`, `startup_failure`, or status `failure`/`error` | Fix the PR or escalate; never merge |
| `SKIP` | Conclusion `skipped`, `neutral`, or cancelled, or status `neutral` | Allowed only per the allowlist below |
| `UNKNOWN` | Anything else | Treat as `PENDING`; escalate if it persists |

Bot checks are not special: Dependabot, CodeQL, secret scanning, and every other
app are classified the same way. "All checks pass including bots" means every
non-allowed `FAIL`/`PENDING`/`UNKNOWN` blocks the merge.

## Skip allowlist

A `SKIP` verdict is acceptable only when one of these holds:

1. The check name appears in `.git/pr-triage/checks-allow.txt` (one name per
   line, `#` comments). The user owns this list.
2. The workflow is a known path-filtered job and the PR does not touch the paths
   the job guards (for example a docs job on a code-only PR, or Dependabot's
   no-change status).

Anything else, including cancelled checks and `action_required`, is not a skip
and must be fixed or escalated. Never invent a skip reason.

## Waiting

Poll `scripts/checks.sh` while the verdict is `PENDING`. Use a bounded wait
(default 30 minutes, then escalate). A check that never reports is `UNKNOWN`;
do not merge around it.
