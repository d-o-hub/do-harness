---
name: skill-creator
description: >
  Create or update agent skills for this repository with the local layout
  (SKILL.md plus hermetic evals/evals.json and evals/walkthrough.sh) and
  repo invariants. Use when scaffolding a new skill in .agents/skills/,
  editing an existing skill, or wiring its evaluation and structure gate.
license: Apache-2.0
metadata:
  short-description: Create or update a skill
---

# Skill Creator

Scaffold and evolve skills in `.agents/skills/` so they pass the structure
gate (`scripts/quick_validate.py`) and the hermetic evaluator
(`do-harness eval`). Read `AGENTS.md` §5 first: skills must obey the same
repo invariants as production code.

## Local Skill Layout

```
<skill-name>/
├── SKILL.md (required: frontmatter name + description, imperative body)
├── evals/evals.json (required: cases with graded assertions)
├── evals/walkthrough.sh (required when residue is graded: hermetic, exit 0)
├── scripts/ (only deterministic, repeatedly-rewritten executables)
└── references/ (detail loaded on demand; SKILL.md links each file)
```

Do not add READMEs, changelogs, or setup docs. Frontmatter allows only
`name`, `description`, `license`, `allowed-tools`, `metadata`.

## Repo Invariants (Non-Negotiable)

- 500 LOC ceiling per file; split SKILL.md via `references/` before 450.
- Rust: `#![forbid(unsafe_code)]`, `#[serde(deny_unknown_fields)]` on
  config/event types, `thiserror` in libraries / `anyhow` in binaries,
  no `unwrap()` in library code.
- Commits: conventional commits, lowercase subject.
- Evals are executable evidence: every `evals.json` assertion that counts
  uses a graded prefix (`exists:`, `absent:`, `contains:`, `db:`, `cli:`,
  `walk:`). Bare prose strings are docs and score nothing — delete them.

## Degrees of Freedom

Match specificity to fragility. Fragile sequences (gate commands, bless
flow, walkthrough env contract) get exact scripts; judgment calls
(trigger wording, heuristic generalization) get concise guidance.

## Process

1. Scaffold: `scripts/init_skill.py <skill-name> --path .agents/skills --resources scripts,references`.
2. Write SKILL.md: imperative, under 5k words; one level of references,
   each linked with when-to-read; grep patterns for files over 100 lines.
3. Write evals: cover explicit, implicit, contextual, and negative cases.
   Every case needs graded assertions; at least one negative case must
   prove the skill stays unloaded (`absent:` plus a proof file).
4. Write `walkthrough.sh` (when residue is graded): `#!/usr/bin/env bash`
   with `set -euo pipefail`; `cwd` and `DO_HARNESS_ROOT` are the sandbox
   root; `DO_HARNESS_BIN` is the binary under test; never touch outside
   `$DO_HARNESS_ROOT`; exit 0 only on real success with stderr tail
   carrying the cause on failure.
5. Validate: `scripts/quick_validate.py <skill-dir>`, then
   `do-harness eval --skill <name>`; re-bless graders explicitly:
   `do-harness eval --bless --skill <name>`. Fixtures are
   executor-agnostic: `eval --agent-cmd <command>` runs an agent per case
   and grades the same assertions, which measures true Skill Lift.
6. Iterate from eval evidence. New reusable patterns go through
   `.agents/skills/skill-distiller`, never straight into prose.

## References

- UI metadata (`agents/openai.yaml`): see
  [references/openai_yaml.md](references/openai_yaml.md).
- Operating invariants and workflow: `AGENTS.md`.
- Assertion DSL semantics: `crates/do-harness/src/eval_assert.rs`.

## Gotchas

- The skill body loads only after triggering: put all when-to-use
  coverage in the frontmatter `description`, never in a body section.
- Never distill a fix that did not pass computational sensors.
- Strip secrets and machine-specific paths from traces and residue.
