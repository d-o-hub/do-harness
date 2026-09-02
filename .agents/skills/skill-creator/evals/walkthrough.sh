#!/usr/bin/env bash
# skill-creator walkthrough: proves the canonical structure gate and the
# initializer are deterministic — quick_validate passes on a valid skill,
# init_skill scaffolds a new skill that also passes, and invalid frontmatter
# is rejected. Leaves graded residue for the assertions.
set -euo pipefail
root="${DO_HARNESS_ROOT:?DO_HARNESS_ROOT required}"

# Valid skill must pass the gate (use skill-creator itself, since sandbox only mirrors that skill).
python3 "$root/.agents/skills/skill-creator/scripts/quick_validate.py" "$root/.agents/skills/skill-creator" >/dev/null

# Scaffold a new skill via init_skill.py and validate it.
tmp_dir="$root/tmp_skills"
rm -rf "$tmp_dir"
mkdir -p "$tmp_dir"
python3 "$root/.agents/skills/skill-creator/scripts/init_skill.py" eval-test-skill --path "$tmp_dir" --resources scripts >/dev/null
# Fix placeholder description to a valid string (template uses TODO list)
python3 - << 'PY' "$tmp_dir/eval-test-skill/SKILL.md"
import pathlib, sys
p = pathlib.Path(sys.argv[1])
t = p.read_text()
t = t.replace(
    "description: [TODO: Complete and informative explanation of what the skill does and when to use it. Include WHEN to use this skill - specific scenarios, file types, or tasks that trigger it.]",
    "description: Test skill for evaluating skill creation flow and validation workflows"
)
p.write_text(t)
PY
python3 "$root/.agents/skills/skill-creator/scripts/quick_validate.py" "$tmp_dir/eval-test-skill" >/dev/null

# Graded residue: proves both steps succeeded.
touch "$root/skill_creator_residue"
cat "$tmp_dir/eval-test-skill/SKILL.md" > "$root/skill_creator_init_proof"

# Invalid frontmatter must fail the gate (fail-closed).
invalid_dir="$tmp_dir/invalid-skill"
mkdir -p "$invalid_dir"
cat > "$invalid_dir/SKILL.md" << 'EOF'
---
name: BAD_NAME
description: test
---
# Bad
EOF
if python3 "$root/.agents/skills/skill-creator/scripts/quick_validate.py" "$invalid_dir" >/dev/null 2>&1; then
  echo "invalid skill should have failed validation" >&2
  exit 1
fi

rm -rf "$tmp_dir"
