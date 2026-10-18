---
name: skill-creator
description: Scaffold and update agent skills with layout and invariants.
license: Apache-2.0
metadata:
  short-description: Create or update a skill
---

# Skill Creator

Scaffold and evolve skills in `.agents/skills/` passing structure gate (`scripts/quick_validate.py`) and evaluator (`do-harness eval`).

## Layout
```
<skill-name>/
├── SKILL.md
├── evals/evals.json
├── evals/walkthrough.sh
├── scripts/
└── references/
```

## Rules
- 500 LOC per file ceiling.
- Frontmatter allows: `name`, `description`, `license`, `allowed-tools`, `metadata`.
- Evals: use graded assertion prefixes (`exists:`, `absent:`, `contains:`, `db:`, `cli:`, `walk:`).
- `quick_validate.py` requires hyphen-case naming.
- Scaffold via `scripts/init_skill.py <skill-name> --path .agents/skills --resources scripts,references`.
