#!/usr/bin/env bash
# Single-source check: every file under crates/do-harness/templates/skills must
# be byte-identical to its canonical counterpart under .agents/skills.
#
# The direction is one-way: `.agents/skills` is the source of truth (those are
# the skills the harness evaluates); `templates/skills` holds the scaffolded
# subset embedded via include_str!. Repo-only skill files (e.g.
# fail-closed-proxy and the development-methodology skills) are intentionally
# not scaffolded and are not checked.
#
# It also fails when a template SKILL.md names a bundled resource
# (`references/*.md`, `scripts/*.py|*.sh`) that the template skill directory
# does not ship. Prose example paths that the canonical skill does not bundle
# are ignored; a dropped canonical resource fails.
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
templates="$repo_root/crates/do-harness/templates/skills"
canonical="$repo_root/.agents/skills"

if [[ ! -d "$templates" ]]; then
  echo "check-skills-sync: missing $templates" >&2
  exit 1
fi
if [[ ! -d "$canonical" ]]; then
  echo "check-skills-sync: missing $canonical" >&2
  exit 1
fi

failures=0
while IFS= read -r -d '' file; do
  rel="${file#"$templates"/}"
  if [[ ! -f "$canonical/$rel" ]]; then
    echo "check-skills-sync: .agents/skills/$rel is missing (template-only file)" >&2
    failures=1
  elif ! cmp -s "$file" "$canonical/$rel"; then
    echo "check-skills-sync: drift: templates/skills/$rel != .agents/skills/$rel" >&2
    failures=1
  fi
done < <(find "$templates" -type f -print0)

# Referenced-resource existence: a relative resource path named by a template
# SKILL.md must exist under that template skill directory whenever the
# canonical skill bundles it. Illustrative example paths (e.g.
# `scripts/rotate_pdf.py`) are not bundled canonically and are skipped.
while IFS= read -r -d '' md; do
  skill_dir="${md%/*}"
  rel_dir="${skill_dir#"$templates"/}"
  refs="$(grep -oE '(references|scripts)/[A-Za-z0-9._/-]+' "$md" | sort -u || true)"
  if [[ -z "$refs" ]]; then
    continue
  fi
  while IFS= read -r ref; do
    if [[ -f "$skill_dir/$ref" ]]; then
      continue
    fi
    if [[ -f "$canonical/$rel_dir/$ref" ]]; then
      echo "check-skills-sync: templates/skills/$rel_dir/SKILL.md references $ref but the file is missing" >&2
      failures=1
    fi
  done <<< "$refs"
done < <(find "$templates" -name 'SKILL.md' -type f -print0)

if [[ "$failures" -ne 0 ]]; then
  echo "check-skills-sync FAIL: sync templates from .agents/skills" >&2
  exit 1
fi
echo "check-skills-sync OK: templates/skills mirrors .agents/skills"
