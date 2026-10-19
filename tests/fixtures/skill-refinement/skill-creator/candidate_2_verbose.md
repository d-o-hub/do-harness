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

# Skill Creator (Comprehensive Authoring & Validation Manual)

## Purpose & Scope
The Skill Creator skill governs the complete lifecycle of agent capabilities within the repository. It standardizes how skills are scaffolded, structured, validated, evaluated, and maintained over time to satisfy strict repository invariants and quality bars.

## Detailed Skill Directory Structure

```
<skill-name>/
├── SKILL.md (required: YAML frontmatter with name and description, imperative body)
├── evals/evals.json (required: test cases with graded assertions)
├── evals/walkthrough.sh (required when residue is graded: hermetic, exit 0)
├── scripts/ (deterministic executables only)
└── references/ (supplementary documentation loaded on demand)
```

### Strict Frontmatter Contract
Skill frontmatter must strictly conform to allowed keys: `name`, `description`, `license`, `allowed-tools`, and `metadata`. Extra keys will cause validation failures under `quick_validate.py`. Skill names must be formatted in strict hyphen-case.

## Repository Invariants & Quality Principles

1. **Strict File LOC Ceilings**: No source file or guidance document may exceed 500 LOC. Guidance files must be split into `references/` before reaching 450 lines.
2. **Graded Assertions in Evals**: Every evaluation case must contain executable assertions using recognized prefixes (`exists:`, `absent:`, `contains:`, `db:`, `cli:`, `walk:`). Unprefixed strings are docs and carry zero evaluation weight.
3. **Hermetic Walkthrough Execution**: Walkthrough scripts (`walkthrough.sh`) must execute in isolated sandboxes using `DO_HARNESS_ROOT` and `DO_HARNESS_BIN`.
4. **Scaffolding Tooling**: New skills must be scaffolded using `scripts/init_skill.py <skill-name> --path .agents/skills --resources scripts,references`.

## Validation & Evaluation Lifecycle
1. Run local structure gate: `scripts/quick_validate.py <skill-dir>`.
2. Run skill evaluator: `do-harness eval --skill <skill-name>`.
3. Ratchet and re-bless floors: `do-harness eval --bless --skill <skill-name>`.
