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

## The realistic adopter corpus

`python3 scripts/workflow-benchmark.py --corpus benchmarks/workflow-adopter-v1.json
--report-json report.json` runs a second, separately versioned corpus
(`workflow-adopter-v1`). It pairs two real, dependency-free multi-file
build-log CLI projects — a Rust crate (`main/cli/parser/filter/summary`
modules, committed `Cargo.lock`) and a Node package (CLI/parser/filter/summary
plus built-in `node:test`, built with `node --check`) — with eight fixed
scenario classes:

1. multi-file Rust filter fix that must preserve record order and counts;
2. source edit after green must block stale success;
3. Node policy edit (`do-harness.toml`) invalidates previous evidence;
4. identical failures exhaust the attempt cap;
5. alternating failures still exhaust the cap;
6. fresh-process Node resume performs no duplicate effect;
7. a synthetic denied-outside-write leaves its marker absent;
8. an unsupported capability leaves the workspace unchanged and cannot pass.

Each case validates a `{id, project_id, scenario, description,
expected_outcome, grader_id}` record plus optional file/variant overlays;
projects carry `{id, fixture, build_argv, test_argv}`. The graders run the
real build/test commands and probe the resulting CLI. Corpus validation rejects
absolute/escaping/symlink fixture paths, unknown or duplicate ids, shell argv,
and malformed patch references. `unsupported` is a first-class adapter exit
action (12) beside `changed` (0), `refused` (10), and `escalated` (11);
`unresolved` expectations assert that no false success is reported.

Adopter attempts are 3 and each output stream is capped at 64 KiB. The wall
budget is derived from a recorded zero-model-cost pilot: `ceil(2 * max pilot
end-to-end seconds)` clamped by the profile cap (120 s standard, 600 s
adopter); exceeding the adopter cap blocks instead of silently raising the
limit. The pilot measurements and the resulting budget are stored in the
report.

## What fixture results mean

Fixture workflows replay fixed patches. They demonstrate the measurement and
regression contract; their different first-attempt rates are scripted examples,
not evidence that a skill improves an agent. Timing is measured for the
adopter corpus (setup/build/verify/end-to-end) and explicitly unavailable in
standard fixture reports so identical inputs produce identical JSON. The report
includes commit, corpus/grader hashes, interpreter/platform, sample size,
success/failure, first-attempt and final rates, regression rate, attempts,
repair cycles, verification cost, cancellations, false successes, missed
planted regressions, expected-unresolved outcomes and per-arm identities.
Wilson 95% intervals describe sampling uncertainty; six tasks cannot establish
a general effect.

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

Each invocation reads one JSON object from stdin: `protocol_version` (2),
opaque `case` identifier, synthetic `task`, `workspace`, `attempt`, fixed
grader feedback, and the resource `budget`. Change that workspace, then exit
0 for a code change, 10 for refusal, 11 for escalation, or 12 for an
unsupported capability. The adapter receives no reference patches or expected
result. A refusal/escalation/unsupported outcome succeeds only when expected
and the workspace is unchanged. Credentials are excluded from the inherited
environment unless their names are explicitly passed with `--allow-env`;
graders never inherit those allowed secrets. Do not put credentials in argv.

## Usage provenance and per-arm identity

Protocol-2 adapter responses may include a `usage` object whose metrics
(`tokens`, `bytes`, `currency`, `interventions`) each carry a numeric `value`,
a `unit`, and a `provenance` of `adapter-reported`, `provider-observed`,
`estimated`, or `unavailable`, plus optional bounded `observed`/`expected`
sample labels and an explicit `complete_total`. Tokens/bytes must be
non-negative integers, currency finite and non-negative with an explicit unit;
booleans, non-finite values, negative values, and over-budget values are
rejected and reported as reasoned unavailable. Missing usage is never zero:
the arm summary keeps a labelled `partial_subtotal` and leaves `complete_total`
null unless every case reported the metric. Context bytes are labelled as
bytes, never relabelled as tokens, and adapter claims are never relabelled as
provider observations. Deprecated protocol-1 flat fields (`token_cost`,
`currency_cost`, `context_bytes`) are still ingested as `adapter-reported`
only.

Per-arm identities (`client`, `adapter`, `harness`, `model`) carry provenance
from `operator-supplied` (via `--baseline-identity KEY=VALUE[:PROVENANCE]` /
`--treatment-identity ...`), `adapter-reported`, `provider-observed`, or
`unavailable`; the harness revision comes from the repository HEAD. The global
`--model-version` flag is removed. Replay and scripted-client results are
labelled `live_effectiveness: false` with `unqualified_by` reasons; no
efficiency claim is ever made from replay or underpowered samples.

## Preregistered decision rule

The report embeds the fixed design: one-feature contrasts, an identical-
treatment control, counterbalanced order, 20 paired repeats per case/contrast,
95% paired intervals, and no post-hoc exclusions. A material benefit requires
>=10% lower cost per verified success OR >=10 percentage-point fewer
interventions, a verified-success rate no more than 5 points worse, and zero
false successes or missed planted regressions. Hitting the fixed budget early
is INCONCLUSIVE. These are thresholds, not predictions; without a qualifying
live paired run the analysis verdict stays inconclusive and #458 remains open
for live effectiveness.

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
