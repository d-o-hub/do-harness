---
name: skill-creator
description: Guide for creating and updating effective skills. This skill should be used when users want to scaffold a new skill or refine an existing one. Ships the init_skill.py scaffolding script and the quick_validate.py structure gate.
license: MIT
---

# Skill Creator

Scaffolds and validates skill directories under `.agents/skills/`.

## Scripts

- `scripts/init_skill.py` – create a new skill directory with a templated `SKILL.md`.
- `scripts/quick_validate.py` – the canonical structure gate; validates a skill's
  `SKILL.md` frontmatter and returns exit code 0 when the skill is structurally valid.
