# Workflow effectiveness benchmarks

`python3 scripts/workflow-benchmark.py` runs the versioned offline benchmark
corpora with no network, API key, model, or installed harness binary. Two
corpora ship together:

- `benchmarks/workflow-v1.json` — the six-case synthetic corpus (bug fix,
  regression fix, multi-file change, configuration, refusal, escalation). It
  is a fast contract test for the measurement mechanics.
- `benchmarks/adopter-v1.json` — a versioned realistic-adopter corpus backed
  by `benchmarks/fixtures/adopter-v1/`: pinned synthetic Rust
  (`pinned-logger`) and Node (`pinned-api-router`) repositories with real
  build/test configuration and previously passing regression checks. Its
  cases exercise multi-file bug fixes, edit-after-green, policy
  invalidation, bounded escalation, fresh-session resume, refusal, and
  unsupported outcomes. A `manifest.json` pins each fixture tree by whole-tree
  SHA-256.

`--self-test` proves determinism (identical inputs reproduce byte-identical
reports), deliberate-regression detection, output/deadline bounds, shell-safe
adapter argv, private-environment exclusion, usage-payload validation, budget
profile bounds, resume mechanics, per-arm identity reproducibility, and the
adopter graders' planted-failure/false-success detection. The
`workflow-benchmark` sensor runs these checks in normal CI.

## Corpus structure and grading

Inputs, initial fixtures, expected outcomes, control/treatment fixture
patches, budgets, rubric version, and (for the adopter corpus) the predeclared
protocol all live in the corpus file. Expected outcomes map to exit codes:
0 changed, 10 refused, 11 escalated, 12 unsupported. The separate executable
graders (`grade.py`, `grade_adopter.py`) test behavior, including previously
passing checks, outside the agent's workspace; they never read the corpus or
the expected outcome. The adopter corpus also verifies its fixture manifest
against the committed trees before any case runs, and the protocol block
predeclares repeats, counterbalancing, tolerance, exclusions, and the
stopping/decision rules.

## What fixture results mean

Fixture workflows replay fixed patches. They demonstrate the measurement and
regression contract; their different first-attempt rates are scripted examples,
not evidence that a skill improves an agent. Timing is explicitly unavailable
in fixture reports (`elapsed_ms` is null, timing is `unmeasured-fixture`) so
identical inputs produce identical JSON. External-mode reports measure elapsed
and verification wall-clock time (`measured-wall-clock`).

Reports include the commit, corpus/grader hashes, fixture manifest hash,
interpreter/platform/environment, resource limits, sample size,
success/failure, first-attempt and final rates with Wilson 95% intervals,
regression rate, attempts, repair cycles, and verification cost in grader
invocations. The reports also carry the per-arm identity block and usage
provenance described below. Six or seven tasks cannot establish a general
effect.

## Usage, context, and provenance contract

Each case row records `token_cost`, `currency_cost`, `context_bytes`,
`adapter_model`, `usage_reason`, and a `usage_provenance` map. Provenance is
one of:

- `adapter-reported` — the adapter emitted a JSON usage claim that passed
  validation (trusted local executable claim, not an independent proof).
- `operator-supplied` — the model identity was pinned with
  `--model-version`; no numeric usage is claimed.
- `unavailable` — no usable claim was produced; the preserved reason
  (`usage_reason`) explains why.
- `estimated` — reserved for an explicit opt-in estimator; the runner never
  fabricates estimates.
- `provider-observed` — reserved for an independent provider/billing
  observation channel, which is not implemented; when it exists it overrides
  adapter-reported claims.

Context is measured in **bytes** (`context_bytes`), never model tokens. Token
and currency costs are separate fields and are never derived from context
bytes. Numeric claim validation is fail-closed: booleans, NaN, ±inf,
negatives, oversized values (above per-field caps), non-numeric values, and
empty or overlong model strings are rejected with reason `invalid adapter
usage payload`; an unparseable or empty claim yields `provider usage
unavailable in adapter protocol`. An invalid claim contributes nothing to
totals.

Partial coverage stays explicit: the summary token status is `measured` only
when every row has a claim, `partial` when some do, and `unavailable`
otherwise — missing samples are **not** treated as zero cost. Per-arm
`workflow_identity` records client name/version, adapter `argv_digest`,
harness commit/python/os/machine, and model identity plus provenance; it is
reproducible across identical runs, and arms differ whenever their commands
differ.

## Budgets and opt-in profiles

Defaults stay at at most two attempts, thirty wall-clock seconds including
verification, and a 64 KiB output cap per invocation. Corpus validation bounds
wall time at 120 seconds, attempts at 3, and output at 512 bytes–1 MiB; these
caps are never silently raised for ordinary CI. Larger runs are opt-in
through a recorded profile: `--profile experimental-pilot`
(600s wall / 3 attempts / 256 KiB output, recorded from manual pilot cold
builds on 2026-11-21) is accepted only when it lies within its declared
`max_authorized` ceiling (900s / 4 attempts / 512 KiB). Loading validates
profile types, positivity, the `recorded_from` provenance string, and the
authorized ceiling; exceeding it raises `profile exceeds authorized bounds`.

No external model launches by default, in self-tests, or in ordinary CI;
external-agent comparisons run only when both `--baseline-command-json` and
`--treatment-command-json` are supplied.

## Execution and artifact retention

Deterministic runs happen in temporary workspaces seeded from the corpus (or
the pinned fixture repositories). Child stdout/stderr are capped and discarded;
artifacts contain fixed reason codes and measurements only. Reports retain
compact case selection, corpus/grader hashes, pinned identities,
environment/cache metadata, per-run outcomes, and the decision summary. Raw
prompts, source bodies, production conversations, credentials, and arbitrary
child output are excluded. Boundary scenarios use synthetic data and local
fake endpoints. Adapters are trusted local executables, not an OS security
sandbox: hostile detached processes or writes outside the workspace require
external isolation.

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
refusal, 11 for escalation, or 12 for unsupported. The adapter receives no
reference patches or expected result. A refusal/escalation/unsupported
outcome succeeds only when expected and the workspace is unchanged. Credentials
are excluded from the inherited environment unless their names are explicitly
passed with `--allow-env`; graders never inherit those allowed secrets. Do not
put credentials in argv. A model identity can be pinned with `--model-version`;
when an adapter reports a validated model string, it overrides that pin and the
provenance is recorded as `adapter-reported`.

Timeout, excessive output, or exhausted attempts escalate the unresolved case.
POSIX children run in a process group terminated at cleanup; Windows uses
`SystemRoot/System32/taskkill.exe /T /F`, falling back to the direct child if
that utility is unavailable. Interrupted resume cases persist their workspace,
attempts, feedback, and remaining wall budget across fresh sessions; the
shared wall budget is never renewed.

## Recorded experiment (2026-11-21) and recommendation

- **Selection**: the committed `adopter-v1` corpus as generated by
  `gen_adopter_fixtures.py` (determinism verified with `--check`); fixture
  trees pinned by `manifest.json` (seed 4581).
- **Predeclared rule**: 3 repeats, alternating per-case pair order, effect
  tolerance 0.15, minimum effect 0.2, planned 18 runs per arm, exclusions for
  early-unsupported cases; decision go/no-go/inconclusive from the 95% delta
  interval.
- **Identities**: harness commit `4f31a1a` (base), Python 3.14.4, Linux
  x86_64; corpus SHA-256 `2aee41f3...`; adopter grader (`grade_adopter.py`)
  SHA-256 `2524f5af...`; fixture manifest SHA-256 `0c6f0f8a...`;
- **Artifacts**: one JSON report per run (e.g.
  `.do-harness/logs/adopter-fixture-suite.log`); no prompts, sources, or
  child output retained.
- **Outcomes**: deterministic fixture mode only — all 7 cases green in both
  arms (7/7 final success, 0 regressions, 8 attempts total, 15 grader
  invocations). No real-agent observations were recorded; the report makes no
  performance or token-saving claim.
- **Decision**: inconclusive — fixture mechanics are validated, but no
  treatment effect was measured and none is claimed. Go/no-go for completion
  enforcement, bounded recovery, and task-resume features remains open.
- **Recommendation / next step**: run the documented opt-in paired
  external-agent experiment (same corpus, protocol, and budgets) with pinned
  agent/model/skill revisions, then publish the report with the predeclared
  decision. Until then, fixture replay is evidence of measurement mechanics
  only.

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
