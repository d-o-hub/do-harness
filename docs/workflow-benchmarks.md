# Workflow effectiveness benchmarks

`python3 scripts/workflow-benchmark.py` runs the versioned
[workflow-v1 corpus](../benchmarks/workflow-v1.json) offline, with no network,
API key, model, or installed harness binary. `--self-test` also proves that a
deliberate regression fails, fixture reports repeat byte-for-byte, output and
deadlines are bounded, and private child output never enters result artifacts.
The `workflow-benchmark` sensor runs these deterministic checks in normal CI.

The six cases cover an isolated bug, a regression fix, a multi-file change,
configuration, refusal and escalation. Inputs, initial fixtures, expected
outcomes, control/treatment fixture patches, budgets and rubric version live
together in the corpus. The separate executable graders test behavior,
including previously passing checks, outside the agent's workspace.

## What fixture results mean

Fixture workflows replay fixed patches. They demonstrate the measurement and
regression contract; their different first-attempt rates are scripted examples,
not evidence that a skill improves an agent. Timing is explicitly unavailable
in fixture reports so identical inputs produce identical JSON. The report
includes commit, corpus/grader hashes, interpreter/platform, sample size,
success/failure, first-attempt and final rates, regression rate, attempts,
repair cycles and verification cost in grader invocations. Wilson 95% intervals
describe sampling uncertainty; six tasks cannot establish a general effect.

## Explicit external-agent comparison

Supply both `--baseline-command-json` and `--treatment-command-json`, each a
JSON argv list. No shell parses them. For a skill comparison, point the first
at an adapter configured without the skill, and the second at the same agent
and model with only that pinned skill changed. Both adapters receive fresh
copies of identical inputs and the same resource budgets. Paired run order
alternates. No external agent runs by default, in self-tests, or in ordinary CI.
An optional protected scheduled job may invoke the same command explicitly.

For example, an adapter that implements a `--skill` selector can compare its
unmodified agent against a pinned skill checkout (replace these paths):

<!-- example: id=workflow_benchmark_skill_pair classification=parser-check-only reason="Explicit external adapter example; no model or network runs in documentation tests" -->
```bash
python3 scripts/workflow-benchmark.py \
  --baseline-command-json '["/path/to/agent-adapter","--skill","none"]' \
  --treatment-command-json '["/path/to/agent-adapter","--skill","/path/to/pinned-skill"]'
```

Each invocation reads one JSON object from stdin: `protocol_version` (1),
opaque `case` identifier, synthetic `task`, `workspace`, `attempt`, and fixed
grader feedback. Change that workspace, then exit 0 for a code change, 10 for
refusal or 11 for escalation. The adapter receives no reference patches or
expected result. A refusal/escalation succeeds only when expected and the
workspace is unchanged. Credentials are excluded from the inherited environment
unless their names are explicitly passed with `--allow-env`; graders never
inherit those allowed secrets. Do not put credentials in argv.

Each case/workflow gets at most two attempts and thirty wall-clock seconds,
including verification. Both output streams are capped at 64 KiB each and
discarded; artifacts contain fixed reason codes and measurements only.
Timeout, excessive output, or exhausted attempts escalate the unresolved case.
POSIX children run in a process group terminated at cleanup; Windows uses
`taskkill /T /F`. Adapters are trusted local executables, not an OS security
sandbox: hostile detached processes or writes outside the workspace require
external isolation. Use synthetic data only.

External reports measure elapsed and verification time. Token/currency cost
and provider/model version remain unavailable because the adapter protocol
does not expose independently trustworthy provider usage. Record the pinned
agent/model/skill versions with the experiment before comparing reports;
argv hashes alone do not identify a remote model revision.

## Regression policy and adding cases

Deterministic fixture or grader regressions block CI. External results are
descriptive trend reports and do not fail on an arbitrary one-run rate.
Compare repeated paired samples with the same corpus, grader, environment,
budget and mode. Predeclare a tolerance and sufficient sample size before
looking at outcomes; report confidence intervals and inconclusive results.

Add an isolated synthetic fixture, task, independent behavioral grader,
previously passing regression checks and expected outcome. Version the corpus
and rubric when they change; never alter scoring after seeing results. Keep
held-out cases separate from skill authoring and agent prompts, include failed
runs, and publish the complete case selection. A known training corpus cannot
prove generalization. Compact JSON artifacts contain no prompts, source files,
production conversation/tool data or child logs.
