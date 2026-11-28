# Verification timeline triage study, protocol v1

Measure whether the offline verification timeline helps maintainers identify
the blocking check and locate the evidence for their conclusion. Compare the
normal `task context` and `status` workflow for local tasks, or `ci-explain`
for CI, with the same workflow plus the timeline.

## Freeze the cases

Collect 12 distinct, sanitized real investigations in the following slots.
The slots identify the required disposition. They do not identify completed
case captures.

| Case IDs | Required disposition |
| --- | --- |
| `rerun-01` through `rerun-04` | A failed attempt followed by a fresh passing rerun |
| `stale-01` through `stale-04` | Evidence is stale and has no fresh replacement |
| `blocked-01` through `blocked-04` | Work remains blocked or the required evidence is missing |

For each slot, record the source incident and pinned repository revision, the
verified CLI version, the workflow used, the ordered attempt files and status
snapshots, and their SHA-256 digests. Record the intended blocking checks, the
exact passing artifact digest for a resolved case, or the reason the case is
unresolved. Two maintainers must confirm each answer against the source before
reviewers receive the case. Use replayed sanitized workspaces when the original
artifacts contain private data, and capture both arms from the same replay.
Exclude cases whose verdict or artifact attribution cannot be reproduced.

Within each disposition, pair cases `01` and `02`, then `03` and `04`. Match
pairs on CI versus local workflow and on the number of attempts and blocking
checks. Freeze the manifest, answer key, and artifact digests in a commit
before timed runs. Retain the answer key separately from the reviewer packets.

## Run the comparison

Recruit six maintainers who did not prepare the cases. Each reviewer diagnoses
all 12 cases, six per arm. Reviewers cannot see another reviewer's answers.
Within each matched pair, assign one case to each arm. Use four separate
assignment orders for the six matched pairs, alternating the first arm and
which case it receives. Assign two reviewers each to orders A and C, and one
reviewer each to orders B and D. Use these sequences:

| Order | Odd-numbered pairs | Even-numbered pairs |
| --- | --- | --- |
| A | First case baseline, second case timeline | First case timeline, second case baseline |
| B | Second case timeline, first case baseline | Second case baseline, first case timeline |
| C | First case timeline, second case baseline | First case baseline, second case timeline |
| D | Second case baseline, first case timeline | Second case timeline, first case baseline |

Rotate the starting pair by one position for each reviewer. Apply each order
to the original pair number after rotation. This gives each case three
diagnoses per arm and each reviewer alternating first arms. A reviewer sees
each incident once.

Start the timer when the reviewer opens the case. Stop when they submit the
diagnosis and its supporting artifact or evidence of the unresolved state.
Allow six minutes per case. Both arms get the same CLI, CI data, raw artifacts,
and snapshots. Give the timeline arm an already-rendered report, and measure
the time to capture the evidence and render the report separately. Record any
extra setup work the baseline needs separately as well. Keep access, cache
conditions, and machine settings comparable.

## Score and decide

A correct submission identifies every expected blocking check and cites the
exact passing artifact digest or supports the expected unresolved disposition.
A grader compares anonymized submissions with the frozen answer key. Record
wrong answers, partial answers, timeouts, and infrastructure failures. Treat
wrong answers and timeouts as unsuccessful. Publish each arm's sample count,
correct count, timeout count, and median time among correct submissions. Also
report the median across all submissions, charging the six-minute cap to
timeouts and incorrect answers, and show evidence capture and render time.

Use the median within-case difference in capped time as the primary estimate.
Add timeline-specific collection and rendering overhead to the timeline arm
once per case before comparing, and report the unadjusted estimate too.
Report a 95% bootstrap interval resampled over distinct cases, and describe
that it summarizes this case sample. Report correctness differences in
percentage points.

- **Adopt** if the estimated time saving is at least 60 seconds, the interval
  excludes zero, and accuracy declines by at most 5 percentage points.
- **Stop** if the interval excludes a saving of 60 seconds and observed accuracy
  does not improve, or observed accuracy declines by more than 5 points.
- **Revise** if coverage is complete but neither threshold settles the decision.
  Record which dispositions exposed a report gap and which were handled by
  the ordinary CLI alone.
- **Inconclusive** if any disposition has fewer than four eligible cases or
  either arm has fewer than 36 recorded diagnoses, including timeouts.

Publish the case manifest, anonymized observations, per-arm summaries, and
decision together. This protocol measures human triage. No timed observations
have been collected under it yet.
