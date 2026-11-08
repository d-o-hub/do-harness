# Unattended jobs

`do-harness job` runs one explicitly authorized task through an external
executor and treats the repository's own verification as the only proof of
completion. It exists so routine work can proceed without a human prompt while
remaining bounded, interruptible, and honest about failure.

## Spec

A job is described by a TOML spec passed with `--spec`. Unknown keys are
rejected, and every limit is finite.

```toml
schema-version = 1
task = "make the failing sensor pass"
revision = "4f2a9c1"          # any revision that resolves to a commit
signal-set = "verification"
attempts = 3                  # total, across resumes
deadline-seconds = 3600       # whole job, measured from first start
executor-timeout-seconds = 900

[executor]
argv = ["dsh", "run", "--task", "TASK.md"]
allowed-env = ["DSH_TOKEN"]   # the only credentials the executor receives
accounting = "unavailable"    # or "reported" when spend is enforceable
result-required = true

[isolation]
mechanism = "container"       # the external process/network/filesystem boundary
```

The spec is operator input, not repository content. Changing any semantic field
changes the deterministic job id, so a modified spec starts a new job instead of
mutating an existing one.

## Isolation boundary

The job creates a disposable git worktree from the pinned commit under
`.do-harness/jobs/<id>/workspace` and runs the executor there. That isolates the
*checkout*: the executor never writes into your working tree.

A git worktree is **not** a process, network, or filesystem sandbox. The spec
must therefore name the external mechanism that provides containment, such as a
container runtime or a sandbox wrapper. `do-harness` records that declaration
and never claims to verify it; `git-worktree` and `none` are rejected. The CLI
terminates the executor's process group when a per-attempt budget expires and
bounds its wait for a killed executor's descendants to release captured output,
so a missed descendant cannot extend an attempt. Descendant cleanup beyond that
boundary belongs to the mechanism you declare.

## Credentials

The executor's environment is cleared. It receives a small base set (`PATH`,
`HOME`, temporary-directory and locale variables) plus exactly the names in
`allowed-env`. A credential that is not declared cannot leak into repository
code. Missing authorization stops the job with
`authorization-unavailable`; the CLI never widens access on its own.

## Budgets and progress

`attempts` and `deadline-seconds` are total budgets. Resume continues from the
persisted counters and the original start time, so an interruption cannot renew
either. A declared repair is applied at most once per job, recorded in an
idempotency ledger before it runs.

Progress is measured from the *set* of failing and unrunnable sensors, never
from diagnostic text. An attempt must strictly shrink the failure set to count
as progress; a repeated signature, a changed set that is not a subset, or
alternating failures all stop the job. The attempt and time caps apply
regardless, so rewording an error cannot buy more budget.

## The gate

An executor exiting `0`, writing `done`, or omitting its result file proves
nothing. After every attempt the job runs the repository's own
`verify --set <signal-set> --strict --record --unchanged=run` and then requires
`status` to report `green`. Stale, missing, red, or vacuous evidence is not
success, and a malformed executor result is recorded rather than trusted.

## Mechanical repairs

Project policy may declare deterministic repairs in `do-harness.toml`:

```toml
[[repairs]]
sensor = "fmt"
argv = ["cargo", "fmt", "--all"]
```

A repair runs only when its named sensor failed in the same attempt, at most
once per job, and only in the job workspace. Semantic work stays the external
executor's responsibility.

## Blocked outcomes

Every stop is durable and typed. `job status` and the receipt record the code,
a bounded redacted diagnostic, the revisions and declared policy, and what an
operator can do next.

| Code | Meaning |
| --- | --- |
| `attempts-exhausted` | The attempt budget was spent without verified completion. |
| `deadline-exhausted` | The whole-job time budget expired. |
| `no-progress` | Failures repeated, alternated, or stopped shrinking. |
| `verification-failed` | The authoritative gate is not green. |
| `executor-unavailable` | The executor program could not start. |
| `executor-timeout` | The executor exceeded its per-attempt budget and was killed. |
| `invalid-executor-output` | The declared result was missing or malformed. |
| `authorization-unavailable` | The job lacks credentials it must not request itself. |
| `unsupported-capability` | The environment cannot support the declared capability. |
| `ambiguous-requirements` | An operator decision is required before continuing. |

Receipts live under `.do-harness/jobs/<id>/` and are never committed. A verified
job removes its disposable workspace; a blocked job preserves it so the failure
can be inspected.

## Non-goals

There is no always-on daemon, multi-agent scheduler, automatic deployment or
merge authorization, or general-purpose sandbox implementation here.
