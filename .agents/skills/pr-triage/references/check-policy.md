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
gh api repos/{owner}/{repo}/commits/HEAD_SHA/check-runs \
  --jq '.check_runs[] | {id, name, conclusion, annotations_url: .output.annotations_url}'
gh api repos/{owner}/{repo}/check-runs/<check_run_id>/annotations \
  --jq '.[] | {path, start_line, annotation_level, message}'
```

Read `annotations_url` from the failing check run rather than splitting the job
link: the browser link's trailing number is a job number, not the check-run id
(the same trap `gh run rerun --job` documents). A workflow-run failure that
carries no annotations keeps its evidence in the job log:

```bash
gh run view <run_id> --json jobs --jq '.jobs[] | {name, databaseId}'
gh run view --job <databaseId> --log-failed | tail -40
```

The URL's job number does not work with `--job` either; only `databaseId` does.

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

If the re-run fails the same way, distinguish before escalating. Observed on
this repository: a degraded Windows image failed the same install-action step
identically on `gh run rerun --failed`, while one empty retrigger commit landed a
fresh runner where every check passed (main's equivalent jobs were green minutes
before and after). So when the failure names a runner-image problem — for
example the install-action `bash startup failure`,
actions/partner-runner-images#169 — push one empty retrigger commit and
re-classify. If the check then fails on the new runner, treat it as real and
escalate: a third attempt buys nothing. A `rerun forbidden` (HTTP 403) response
is plumbing — follow
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

## Sources

- REST: check runs and check-run annotations
  (`docs.github.com/en/rest/checks/runs`; requests carry the documented
  `X-GitHub-Api-Version` header, which `gh api` sets).
- `gh run rerun` manual (`cli.github.com/manual/gh_run_rerun`) for the
  `--failed` semantics and the job-number trap.
