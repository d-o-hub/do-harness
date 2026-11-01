#!/usr/bin/env bash
# check-repo-links.sh — the CLI must work for every GitHub repository.
#
# Sensor: scripts/check-repo-links.sh
#
# A tool that names its own repository where the *current* repository is meant
# is wrong in every adopting checkout: the failure is silent, because the string
# is a plausible link rather than a crash. This guard fails when the literal
# `d-o-hub` appears in a surface that ships to a consumer — the CLI sources, the
# scaffolded templates, and the release scripts — outside the small identity
# allowlist below.
#
# An identity link is the tool's own address (its docs, its install script, its
# published source). It is not a stand-in for the repository the CLI is running
# inside: that value must be derived (gh repo view --json nameWithOwner, or the
# origin remote) and the command must fail closed when it cannot be.
#
#   bash scripts/check-repo-links.sh            # scan this checkout
#   bash scripts/check-repo-links.sh --self-test  # hermetic can-fail proof
#
# Scope: `crates/*/src/**/*.rs` except test modules, `crates/do-harness/
# templates/**`, and `scripts/*.sh`. Out of scope by design: this project's own
# documentation (`README.md`, `docs/`, `plans/`, `AGENTS.md`, `man/`,
# `completions/`, `.github/`), test fixtures, and test code — they describe or
# exercise the tool rather than ship as its behaviour.
set -euo pipefail

# shellcheck disable=SC1091 # resolved at runtime from this script's directory
source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/lib.sh"

repo_root="$(dh_repo_root)"

# Files where `d-o-hub` is the tool's identity, not an assumption about the
# caller's repository. Keep this list small; a new entry needs a reason here.
ALLOWED=(
  # The crate's own docs.rs landing link.
  "crates/do-harness/src/lib.rs"
  # The install line an adopting repository needs to install the tool at all.
  "crates/do-harness/templates/AGENTS.md"
  # Pointer to the assertion grammar that ships with the tool.
  "crates/do-harness/templates/skills/skill-creator/SKILL.md"
  # `DO_HARNESS_REPO` overridable default for release downloads.
  "scripts/install.sh"
  # `${GITHUB_REPOSITORY:-…}` fallback inside a remediation message.
  "scripts/publish-npm.sh"
  # Provenance note in a module doc comment: names where the waiver classes were
  # measured. The string is never emitted and never stands in for a repository.
  "crates/do-harness/src/pr/waivers.rs"
  # Self-test fixture changelogs: synthetic pull-request bullets for the
  # composer, not an assumption about the caller's repository.
  "scripts/release-notes.sh"
  # This check, whose comments and self-test fixtures name the literal it hunts.
  "scripts/check-repo-links.sh"
)

self_test=0
case "${1:-}" in
  --self-test)
    self_test=1
    ;;
  "")
    ;;
  *)
    echo "usage: check-repo-links.sh [--self-test]" >&2
    exit 2
    ;;
esac

allowed() {
  local candidate="$1" entry
  for entry in "${ALLOWED[@]}"; do
    [[ "$candidate" == "$entry" ]] && return 0
  done
  return 1
}

scan() {
  local root="$1" failures=0 file rel
  while IFS= read -r -d '' file; do
    rel="${file#"$root"/}"
    allowed "$rel" && continue
    if grep -q 'd-o-hub' "$file"; then
      echo "check-repo-links: $rel names d-o-hub; derive the current repository instead" >&2
      grep -n 'd-o-hub' "$file" | sed 's/^/  /' >&2
      failures=$((failures + 1))
    fi
  done < <(
    {
      find "$root/crates" -path '*/src/*' -name '*.rs' \
        ! -path '*/src/tests/*' ! -path '*/src/*/tests/*' \
        ! -name '*_tests.rs' -print0
      find "$root/crates/do-harness/templates" -type f -print0
      find "$root/scripts" -maxdepth 1 -name '*.sh' -print0
    } 2>/dev/null
  )
  if ((failures > 0)); then
    echo "check-repo-links FAIL: $failures surface(s) assume this repository" >&2
    return 1
  fi
  echo "check-repo-links OK: no shipped surface assumes this repository"
}

run_self_test() {
  local work failures=0
  work="$(mktemp -d)"
  trap 'rm -rf "$work"' RETURN

  mkdir -p "$work/crates/demo/src" "$work/crates/demo/templates" "$work/scripts"
  # 1. A shipped source that assumes the repository must fail.
  printf 'let repo = "d-o-hub/do-harness";\n' >"$work/crates/demo/src/leak.rs"
  if scan "$work" >/dev/null 2>&1; then
    echo "self-test: a hardcoded repository in a shipped source was NOT rejected" >&2
    failures=$((failures + 1))
  fi
  # 2. The same string in test code is a fixture, not behaviour.
  rm "$work/crates/demo/src/leak.rs"
  printf 'let repo = "d-o-hub/do-harness";\n' >"$work/crates/demo/src/fixtures_tests.rs"
  mkdir -p "$work/crates/demo/src/tests"
  printf 'let repo = "d-o-hub/do-harness";\n' >"$work/crates/demo/src/tests/cases.rs"
  if ! scan "$work" >/dev/null 2>&1; then
    echo "self-test: test code must be out of scope" >&2
    failures=$((failures + 1))
  fi
  # 3. The declared identity surfaces stay allowed.
  rm -rf "$work/crates/demo/src"
  cat >"$work/scripts/install.sh" <<'SH'
REPO="${DO_HARNESS_REPO:-d-o-hub/do-harness}"
SH
  if ! scan "$work" >/dev/null 2>&1; then
    echo "self-test: an allowlisted identity surface must pass" >&2
    failures=$((failures + 1))
  fi
  # 4. A deriving surface passes without exempting anything.
  rm "$work/scripts/install.sh"
  cat >"$work/scripts/derive.sh" <<'SH'
repo="$(gh repo view --json nameWithOwner --jq .nameWithOwner)"
SH
  if ! scan "$work" >/dev/null 2>&1; then
    echo "self-test: a deriving surface must pass" >&2
    failures=$((failures + 1))
  fi

  if ((failures > 0)); then
    echo "check-repo-links: self-test FAILED" >&2
    return 1
  fi
  echo "check-repo-links: self-test passed (leak rejected, tests and identity surfaces allowed)"
}

if ((self_test)); then
  run_self_test
else
  scan "$repo_root"
fi
