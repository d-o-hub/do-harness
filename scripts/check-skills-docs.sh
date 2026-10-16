#!/usr/bin/env bash
# check-skills-docs.sh — drift gate between the skill catalog and its docs.
#
# Sensor: scripts/check-skills-docs.sh
# What it proves: `docs/skills.md` names exactly the skills that exist under
# `.agents/skills/` (no missing, no stale), every repo-relative path the docs
# name exists on disk, and `README.md` points at the guide. Token-cheap by
# design: it reads frontmatter `name:` lines only (never a skill body) and
# compares sorted name sets, so CI pays bytes, not context.
# Enforcement policy (fail-closed always):
#   - drift (missing/stale skill row, dangling path, missing README pointer)
#     exits 1 with an itemized report, locally and in CI.
#   - `--fix` regenerates the catalog table from frontmatter (added rows carry
#     an empty purpose cell for the author to fill; stale rows are dropped)
#     and exits 0. Run it locally after adding, renaming, or removing a skill,
#     then fill any empty purpose cell by hand: purposes are judgment, names
#     are mechanical.
# Portability: POSIX awk/grep/sed/sort only (macOS ships bash 3.2, so no
# associative arrays). Purposes must not contain `|` (it is the column
# separator the `--fix` rewrite splits on).
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

FIX=0
if [[ "${1:-}" == "--fix" ]]; then
    FIX=1
elif [[ $# -gt 0 ]]; then
    echo "check-skills-docs: usage: check-skills-docs.sh [--fix]" >&2
    exit 2
fi

SKILLS_DOC="docs/skills.md"
README="README.md"
failures=0

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

# The skill tree is a required artifact: with no skills the drift check would
# compare two empty sets. nullglob means a missing tree yields an empty array
# (not a literal glob handed to grep), so the guard below is reachable and the
# failure is a clear message rather than a raw grep error.
shopt -s nullglob
skill_files=(.agents/skills/*/SKILL.md)
shopt -u nullglob
if [[ "${#skill_files[@]}" -eq 0 ]]; then
    echo "check-skills-docs FAIL: no skills found under .agents/skills (empty catalog)." >&2
    exit 1
fi

# Canonical names: frontmatter only, one grep per SKILL.md, sorted.
for md in "${skill_files[@]}"; do
    grep -m1 '^name:' "$md" | sed 's/^name: *//'
done | sort -u > "$tmp/names.txt"

# Documented names: backticked table rows (`| `+"`name`"`) in the guide.
grep -oE "^\| \`[^\`]+\`" "$SKILLS_DOC" | sed "s/^| \`//; s/\`$//" | sort -u > "$tmp/docnames.txt"

while IFS= read -r name; do
    if ! grep -qxF "$name" "$tmp/docnames.txt"; then
        echo "check-skills-docs: $SKILLS_DOC is missing skill '$name' (run with --fix)" >&2
        failures=1
    fi
done < "$tmp/names.txt"

while IFS= read -r name; do
    if ! grep -qxF "$name" "$tmp/names.txt"; then
        line="$(grep -nF "| \`$name\` |" "$SKILLS_DOC" | cut -d: -f1 | head -1)"
        echo "check-skills-docs: $SKILLS_DOC:${line:-?} names stale skill '$name' (run with --fix)" >&2
        failures=1
    fi
done < "$tmp/docnames.txt"

# Every repo-relative path the guide (and the README Skills section) names
# must exist. Only restricted prefixes are claims; prose and skill-relative
# paths (evals/..., references/...) are not.
readme_skills="$(awk '/^## Skills$/{flag=1; next} /^## /{flag=0} flag' "$README")"
{
    grep -oE "\`(\.agents|scripts|docs|crates|plans|integrations)/[A-Za-z0-9._/*-]+" "$SKILLS_DOC"
    printf '%s\n' "$readme_skills" | grep -oE "\`(\.agents|scripts|docs|crates|plans|integrations)/[A-Za-z0-9._/*-]+" || true
} | sed 's/^`//' | sort -u > "$tmp/paths.txt"
while IFS= read -r path; do
    if [[ ! -e "$path" ]]; then
        echo "check-skills-docs: documented path '$path' does not exist" >&2
        failures=1
    fi
done < "$tmp/paths.txt"

# README must point at the guide; otherwise the guide is unreachable.
if ! printf '%s\n' "$readme_skills" | grep -q 'docs/skills\.md'; then
    echo "check-skills-docs: $README Skills section does not point at $SKILLS_DOC" >&2
    failures=1
fi

if [[ "$FIX" -eq 1 ]]; then
    # Existing purposes keyed by name (tab-separated staging file).
    grep -E "^\| \`[^\`]+\` \|" "$SKILLS_DOC" \
        | sed "s/^| \`//; s/\` | /\t/; s/ |\$//" > "$tmp/existing.tsv"
    {
        echo "| Skill | Purpose |"
        echo "|-------|---------|"
        while IFS= read -r name; do
            purpose="$(awk -F'\t' -v n="$name" '$1 == n {print $2}' "$tmp/existing.tsv")"
            echo "| \`$name\` | $purpose |"
        done < "$tmp/names.txt"
    } > "$tmp/table.txt"
    awk '
        /^\| Skill \| Purpose \|$/ {
            while ((getline line < table) > 0) { print line }
            close(table); skip=1; next
        }
        skip && /^\|/ { next }
        { skip=0; print }
    ' table="$tmp/table.txt" "$SKILLS_DOC" > "$tmp/skills.md"
    # Stale rows outside the rebuilt table (if any) are dropped so one
    # --fix always converges back to green.
    while IFS= read -r name; do
        if ! grep -qxF "$name" "$tmp/names.txt"; then
            grep -vF "| \`$name\` |" "$tmp/skills.md" > "$tmp/skills.md.tmp"
            mv "$tmp/skills.md.tmp" "$tmp/skills.md"
        fi
    done < "$tmp/docnames.txt"
    if cmp -s "$tmp/skills.md" "$SKILLS_DOC"; then
        echo "check-skills-docs --fix: no changes"
    else
        cp "$tmp/skills.md" "$SKILLS_DOC"
        echo "check-skills-docs --fix: rewrote catalog table in $SKILLS_DOC"
    fi
    exit 0
fi


# Epic lessons are cited by file, never by number. The mcp epic used to write
# "harness heuristic 18" - a counter that lived only in the author's session, so
# the reference resolved to nothing (the file's bullet order never matched it
# either). A citation must name the file, and that file must exist.
for epic in "$ROOT"/plans/*-epic.md; do
    [[ -f "$epic" ]] || continue
    numbered="$(grep -nE 'heuristic [0-9]+' "$epic" || true)"
    if [[ -n "$numbered" ]]; then
        echo "check-skills-docs FAIL: $(basename "$epic") cites a numbered heuristic; cite the file instead:" >&2
        printf '%s\n' "$numbered" >&2
        failures=1
    fi
    while IFS= read -r citation; do
        [[ -n "$citation" ]] || continue
        if [[ ! -f "$ROOT/.agents/skills/$citation" ]]; then
            echo "check-skills-docs FAIL: $(basename "$epic") cites $citation, which does not exist" >&2
            failures=1
        fi
    done < <(grep -oE '[A-Za-z0-9_-]+/references/heuristics\.md' "$epic" | sort -u)
done

if [[ "$failures" -ne 0 ]]; then
    echo "check-skills-docs FAIL: see the items above; the catalog table itself can be rebuilt with --fix (then fill empty purposes by hand)" >&2
    exit 1
fi
echo "check-skills-docs OK: $SKILLS_DOC matches $(wc -l < "$tmp/names.txt" | tr -d ' ') skill(s), all documented paths exist"
