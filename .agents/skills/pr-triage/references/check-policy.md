# Check policy

## Sources and binding

- `do-harness pr checks PR --format json` is the single, typed source of CI
  truth. It binds the verdict to the current `headRefOid`, reads every page of
  check runs and legacy commit statuses, verifies the API's `total_count`, and
  re-reads the head after collection. A head that moved is re-fetched; an
  unstable head is `UNKNOWN`.
- `scripts/checks.sh` is a thin wrapper over that command. It only paces the
  bounded wait and renders the legacy TSV rows; it keeps no second parser.
- `gh pr checks PR` is not bound to a commit and is never the merge authority.
- The report includes the checked head SHA, each check's name, source (app
  slug or `status`), completion state, verdict, and an explicit blocker list.

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
do-harness ci-explain <run_id>   # failing step, local sensor, exact repro command
```

`ci-explain` resolves the failing step, the sensor that reproduces it, and the
newest error line, reading the same log by hand when it cannot:

```bash
gh run view <run_id> --json jobs --jq '.jobs[] | {name, databaseId}'
gh run view --job <databaseId> --log-failed | tail -40
```

The URL's job number does not work with `--job` either; only `databaseId` does.

Then classify:

- **Real failure.** An annotation points at a file the PR changed, or the failing
  step reproduces locally. Fix it; do not re-run it.
- **Cancellation.** Conclusion `cancelled` is not a failure: the job stops on an
  external cause (superseded push, concurrency group, runner loss), usually with
  no failing step and no annotations. Re-run it
  (`gh run rerun <run_id> --job <databaseId>`); never "fix" code for it.
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
| `SKIP` | Conclusion `skipped`/`neutral`, or status `neutral`, and the name is allowlisted | Continue; the skip is recorded with its reason |
| `UNKNOWN` | Anything else, including an unreadable/truncated API, an empty check set without policy, or a missing required check | Never merge; escalate |

Bot checks are not special: Dependabot, CodeQL, secret scanning, and every other
app are classified the same way. "All checks pass including bots" means every
non-allowed `FAIL`/`PENDING`/`UNKNOWN` blocks the merge.

`cancelled` is always `FAIL` (rerun it; never "fix" code for it). An
unallowlisted `skipped`/`neutral` is `FAIL`, not a silent pass.

## Policy

The policy is auditable and lives under `.git/pr-triage/`:

```toml
# .git/pr-triage/checks-policy.toml
allow_no_ci = false          # true: an empty check set is a pass (explicit opt-in)
required = ["verify"]        # names that must be present and passing
allow_skips = ["docs"]       # names whose skipped/neutral conclusion is acceptable
```

`checks-allow.txt` (one name per line, `#` comments) is still honoured as an
additional skip allowlist. A missing policy is the strict default: CI is
expected, no check is required by name, and no skip is allowed. A malformed
policy is an error, never a permissive fallback.

An empty check set is `UNKNOWN` unless `allow_no_ci = true`. A repository with
intentionally no CI must opt in explicitly; the opt-in is the audit trail.

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

The merge boundary is `scripts/merge.sh PR REPORT`: it re-runs the whole
authorization snapshot (`pr ready --expected-receipt`) immediately before the
merge and refuses anything but `MERGE_ALLOWED`. An unavailable binary, an
unreadable API, a pending check, an unknown state, or a receipt whose snapshot
no longer matches is a hard stop.


## Server ruleset protection

Client-side checks are backed by repository ruleset enforcement on the GitHub
server. The server requires:
- Review thread resolution (`required_review_thread_resolution: true`).
- Required status checks: `pr-required` and `semver` (integration ID `15368`).
- Strict status check policy (up-to-date base before merge).
- No bypass actors (`bypass_actors: []`, `current_user_can_bypass: "never"`).

Check drift against `plans/pr-merge-policy.toml` using `do-harness doctor pr-policy`.
## Sources

- REST: check runs and check-run annotations
  (`docs.github.com/en/rest/checks/runs`; requests carry the documented
  `X-GitHub-Api-Version` header, which `gh api` sets).
- `gh run rerun` manual (`cli.github.com/manual/gh_run_rerun`) for the
  `--failed` semantics and the job-number trap.
