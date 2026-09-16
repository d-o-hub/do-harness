---
name: skill-distiller
description: >
  Compress successful interaction traces, compiler error-recovery patterns, and
  spike learnings into reusable, benchmarked skills. Use after a vertical slice
  passes all computational sensors, after recovering from a non-trivial error,
  when the steering loop requires updating a feedforward guide, or when
  creating or updating a skill in .agents/skills/.
license: MIT
metadata:
  version: "0.1.0"
  tags: distillation dreaming skills heuristics eval steering
---
## Guides

See references/heuristics.md for distilled heuristics.


# Skill Distiller & Dreaming Skill

## Purpose
Post-task distillation loop that compresses successful interaction traces, compiler error-recovery patterns, and spike learnings into reusable, benchmarked skills.

## Distillation Trigger
Execute when:
1. A vertical slice successfully passes all computational sensors (`cargo test`, `cargo clippy`).
2. An agent recovered from a non-trivial error (e.g., borrow checker conflicts, async lifetime issues, libSQL driver quirks).
3. A spike resolved a previously unknown architectural constraint.
4. **Steering loop**: the same sensor fired more than 2 times in one sprint — the root cause should become a feedforward guide.

## Execution Steps

### 1. Extract Interaction Trace
- Query `.do-harness/agent_state.db` for the active session's commands, error diffs, and resolution steps.

### 2. Generalize Heuristic
- Convert the specific fix into a generalized pattern.
- Strip project-specific identifiers while preserving the structural solution.

### 3. Update / Create Agent Skill
- If a skill in `.agents/skills/` matches the domain, update its `SKILL.md` with the new heuristic.
- If novel, generate a new directory `.agents/skills/<skill-name>/` with:
  - `SKILL.md`: Core guidance, invariants, and code examples (with Agent Skills frontmatter).
  - `evals/evals.json`: At least 2-3 verification cases with `id`, `prompt`, `expected_output`, and checkable `assertions`.
- Run `do-harness overlap` after any corpus change: new or moved guidance must not push a pair past the accepted baselines in `plans/invariants.json`.

### 4. Benchmark & Evaluate
Run the skill-evaluator loop:
1. **Structure check** — `SKILL.md` present, frontmatter valid, `evals/evals.json` exists and parses.
2. **Eval review** — each case has a real prompt, a concrete expected outcome, and checkable assertions.
3. **Live run** — execute one representative prompt against the skill and grade with evidence.
4. **Baseline comparison** — when measuring improvement, rerun the same prompt without the skill or against the previous version.
5. **Verdict** — `PASS`, `NEEDS_WORK`, or `FAIL` with evidence; iterate until `PASS`.

### 5. Steering Loop
- Canonical rule lives in `.agents/skills/harness` (Steering Loop): a sensor
  firing >2 times in one sprint is a feedforward-guide defect, not a symptom
  to patch. Follow it; do not restate it here.
- If no guide exists, create one in `.agents/skills/` via steps 1-4.

## Gotchas
- Never distill a fix that did not pass computational sensors — hallucinations propagate.
- Strip secrets and machine-specific paths from traces before writing them into a skill.

## Negative Knowledge (Anti-Patterns)

The "only distill verified fixes" rule governs *positive* patterns. Negative
knowledge — a trap that was actually hit and not solved — is equally valuable
and is stored as a `kind: gotchas` eval case, whose assertions prove the wrong
action was **not** taken:

```json
{
  "kind": "gotchas",
  "assertions": [
    "absent:.do-harness/blessed-to-pass",
    "not-contains:recovery-log.md|relaxed the sensor"
  ]
}
```

`eval --strict-fixtures` rejects a `gotchas` case with no negative
(`absent:` / `not-contains:`) assertion, so an anti-pattern cannot be recorded
as a positive-only claim it never earned.

Anti-patterns live in the skill's `## Anti-patterns` section, each naming the
observed wrong action and why it failed. Seed them from real incidents — an
unsolved blocker, a repeated correction, a revert — never from speculation.

The npm publishing work is the seed example. Its four recorded anti-patterns
(per-package non-reusable 2FA approval, no Trusted Publisher before a package
exists, `npm publish --dry-run` not checking name similarity, and never
renaming a single reference or unpublishing live siblings) live in
`.agents/skills/npm-github-publish/SKILL.md`. They are graded with `absent:`
and `not-contains:` assertions that run against the real publisher's output,
so they stay provable without any positive npm fix having landed.

## Measuring Skill Lift (and its limits)

> **Optional and slow.** Nothing below runs in CI or in any gate: the CI eval
> step is `do-harness eval --strict-fixtures` (deterministic walkthroughs, under
> 3s per skill). Agent-lift measurement spawns a real agent per case — minutes,
> not seconds — so run it deliberately when you are changing a fixture or
> auditing a lift number, never as part of a routine loop.
> `do-harness distill --from-strikes` is likewise optional: it writes starter
> scaffolds on demand and is not wired into `verify`, hooks, or CI.

`do-harness eval --skill <name>` reports `lift` = with-skill pass rate minus
without-skill pass rate, where the baseline strips `SKILL.md` + `references/`
before running the same executor.

Three distinct outcomes, do not conflate them:

- `lift=+0.29` — a number. Meaning depends on what the assertions read.
- `lift=contaminated` — the baseline regenerated its own guidance (e.g. the
  walkthrough ran `do-harness init`, which re-scaffolds `SKILL.md`), so the
  subtraction is unusable. Fix the fixture; do not read it as a zero.
- `lift=n/a` — not measured (no baseline, or neither side graded anything).

**A non-zero lift does not prove the agent used the guidance.** Assertions of
the form `contains:.agents/skills/<name>/SKILL.md|...` pass because the skill
file is copied into the sandbox, not because any agent read it. Measured
directly: a stub agent that does nothing but `exit 0`, and a real coding agent,
both score identically on such a fixture — the delta is just the count of
guidance-reading assertions divided by the total. Verify by running the eval
against a do-nothing command before trusting any lift figure:

```bash
eval --skill <name> --agent-cmd 'true'    # null-agent control
eval --skill <name> --agent-cmd <real>    # if equal, lift measures file presence only
```

To measure agent *behavior*, grade the executor's own output (`agent_stdout.txt`
in agent mode, or artifacts the walkthrough creates) rather than files shipped
in the sandbox.

## Strike-Driven Scaffolding

A sensor that strikes 3+ times has no verified fix yet, so distilling from it
positively would fabricate one. Use the recorded signature instead:

```bash
do-harness distill --from-strikes
```

This generates the starter pair (SKILL.md generated from the signature + a
failing `gotchas` fixture) via the same skill-creator layout. The starter is
legitimately red: its `exists:` assertion names the recovery log the author has
not written yet. Completing the guide turns it green.