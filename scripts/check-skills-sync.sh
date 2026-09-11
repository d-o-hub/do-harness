#!/usr/bin/env bash
# Single-source check: every file under crates/do-harness/templates/skills must
# be byte-identical to its canonical counterpart under .agents/skills.
#
# The direction is one-way: `.agents/skills` is the source of truth (those are
# the skills the harness evaluates); `templates/skills` holds the scaffolded
# subset embedded via include_str!. Repo-only skill files (fail-closed-proxy,
# reference notes, and skill-creator extras) are intentionally not scaffolded
# and are not checked.
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

if [[ "$failures" -ne 0 ]]; then
  echo "check-skills-sync FAIL: sync templates from .agents/skills" >&2
  exit 1
fi
echo "check-skills-sync OK: templates/skills mirrors .agents/skills"
