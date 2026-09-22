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

## CI truth: annotations, not badges

A red badge is a signal, not a verdict. Before calling a failing check a code
failure, read what the run actually says:

```bash
gh pr checks PR --json name,state,link   # which check failed, and its job link
gh api repos/{owner}/{repo}/check-runs/<check_run_id>/annotations \
  --jq '.[] | {path, start_line, annotation_level, message}'
```

`<check_run_id>` is the trailing segment of the failing check's job link
(`.../job/106683915433`). A workflow-run failure that carries no annotations
puts its evidence in the job log:
`gh run view --job <job_id> --log-failed | tail -40`.

Then classify:

- **Real failure.** An annotation points at a file the PR changed, or the failing
  step reproduces locally. Fix it; do not re-run it.
- **Infrastructure flake.** Re-run once with `gh run rerun <run_id> --failed`,
  never "fix" it:
  - third-party action authentication breakage (FlakeHub and similar);
  - a hosted runner lost or terminated — `The runner has received a shutdown
    signal`, or a step with empty logs that normally prints output;
  - an external-only red (Codacy, Sonar, another app) while every GitHub check is
    green, with the app's status page as evidence.

If the re-run fails the same way, treat it as real and escalate: a third re-run
buys nothing. A `rerun forbidden` (HTTP 403) response is plumbing — follow
[gh-resilience.md](gh-resilience.md) (one empty retrigger commit, one cycle, then
escalate), never a merge around the check.

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
(default 30 minutes, then escalate). When a webhook receiver is armed
(`--events-url` / `PR_TRIAGE_EVENTS_URL`; see
[webhook-fast-path.md](webhook-fast-path.md)) the wait wakes on
`check_run`/`workflow_run` deliveries and re-classifies, and a safety chunk
(default 300 seconds, `PR_TRIAGE_EVENT_CHUNK`) re-classifies even without
events. Missing deliveries must degrade to polling, never to done. A check
that never reports is `UNKNOWN`; do not merge around it.
