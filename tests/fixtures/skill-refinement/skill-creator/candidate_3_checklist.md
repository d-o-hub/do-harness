---
name: skill-creator
description: Scaffold and update agent skills with layout and invariants.
license: Apache-2.0
metadata:
  short-description: Create or update a skill
---

# Skill Creator Checklist

| Component | Standard | Enforcement Tool |
|---|---|---|
| **Skill Layout** | `SKILL.md`, `evals/evals.json`, `evals/walkthrough.sh` | `quick_validate.py` |
| **Frontmatter** | `name` (hyphen-case), `description`, `license`, `metadata` | `quick_validate.py` |
| **File Length** | <= 500 lines per file (split at 450 to `references/`) | `check-loc.sh` |
| **Evals** | Executable assertions (`exists:`, `absent:`, `contains:`, `db:`, `cli:`, `walk:`) | `do-harness eval` |

## Workflow Steps Checklist
- [ ] Scaffold skill: `scripts/init_skill.py <skill-name> --path .agents/skills --resources scripts,references`.
- [ ] Write `SKILL.md` and `evals/evals.json`.
- [ ] Validate structure: `scripts/quick_validate.py .agents/skills/<skill-name>`.
- [ ] Run evaluation: `do-harness eval --skill <skill-name>`.
