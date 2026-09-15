#!/usr/bin/env bash
# check-npm-sequence.sh — executable guard for the npm release sequence.
#
# Prose ordering in docs/releasing.md drifted twice on the same contradiction:
# it told operators to configure a Trusted Publisher for packages npm will not
# expose settings for until they exist. The order is therefore enforced here
# instead of re-stated in prose:
#
#   bootstrap-publish (per package) -> configure Trusted Publisher -> OIDC CI
#
# Usage:
#   check-npm-sequence.sh --root <repo>   validate a real workspace
#   check-npm-sequence.sh --self-test     negative controls (must be able to fail)
set -euo pipefail

usage() {
  cat <<'USAGE'
usage: check-npm-sequence.sh (--root <repo> | --self-test)

  --root <repo>   validate the release sequence in a workspace
  --self-test     prove each check can fail (mutation controls)
USAGE
}

fail() {
  printf 'FAIL: %s\n' "$1" >&2
  return 1
}

ok() {
  printf 'OK: %s\n' "$1"
}

# Platform package names from the publisher's TARGETS table (3rd field):
# `target:platform:package:archive:binary`.
publisher_packages() {
  local publisher="$1/scripts/publish-npm.sh"
  [[ -f "$publisher" ]] || { fail "missing $publisher"; return 1; }
  sed -n '/^TARGETS=(/,/^)/p' "$publisher" \
    | sed -n 's/^[[:space:]]*"[^:"]*:[^:"]*:\([^:"]*\):.*$/\1/p'
}

# The meta package must publish after the platform loop, so its pinned
# optionalDependencies already exist on the registry.
check_publisher_order() {
  local root="$1" publisher="$1/scripts/publish-npm.sh"
  [[ -f "$publisher" ]] || { fail "missing $publisher"; return 1; }
  local loop meta
  # Anchors are deliberately `$`-free: shellcheck SC2016 rejects quoting a
  # literal `$` in grep patterns, and `^publish_dir ` (column 0) is unique to
  # the meta step because the in-loop call is indented.
  loop="$(grep -nF -m1 'for entry in ' "$publisher" | cut -d: -f1)"
  meta="$(grep -nE -m1 '^publish_dir ' "$publisher" | cut -d: -f1)"
  if [[ -z "$loop" || -z "$meta" ]]; then
    fail "publisher is missing the platform loop or the meta publish step"
    return 1
  fi
  # The meta step must come after the platform loop completes.
  if (( meta <= loop )); then
    fail "meta package is published before the platform packages"
    return 1
  fi
  ok "publisher order: platform packages before the meta package"
}

# Every platform package must be pinned by the meta package, or an install
# silently resolves nothing for that platform.
check_optional_deps() {
  local root="$1" meta="$1/integrations/npm/package.json"
  [[ -f "$meta" ]] || { fail "missing $meta"; return 1; }
  local pkg missing=0
  while IFS= read -r pkg; do
    [[ -n "$pkg" ]] || continue
    if ! grep -q "\"$pkg\"" "$meta"; then
      fail "meta optionalDependencies does not pin $pkg"
      missing=1
    fi
  done < <(publisher_packages "$root")
  (( missing == 0 )) || return 1
  ok "meta optionalDependencies pin every platform package"
}

# The shim resolves platform packages through lib/platform.js; a platform
# package missing there is unreachable at runtime.
check_platform_map() {
  local root="$1" map="$1/integrations/npm/lib/platform.js"
  [[ -f "$map" ]] || { fail "missing $map"; return 1; }
  local pkg missing=0
  while IFS= read -r pkg; do
    [[ -n "$pkg" ]] || continue
    if ! grep -q "\"$pkg\"" "$map"; then
      fail "platform map does not resolve $pkg"
      missing=1
    fi
  done < <(publisher_packages "$root")
  (( missing == 0 )) || return 1
  ok "platform map resolves every platform package"
}

# The twice-corrected contradiction: the runbook must not ask for Trusted
# Publisher configuration before explaining the bootstrap publication.
check_docs_order() {
  local root="$1" docs="$1/docs/releasing.md"
  [[ -f "$docs" ]] || { fail "missing $docs"; return 1; }
  local bootstrap configure
  bootstrap="$(grep -n 'bootstrap publish first' "$docs" | head -1 | cut -d: -f1)"
  configure="$(grep -n 'configure its Trusted Publisher' "$docs" | head -1 | cut -d: -f1)"
  if [[ -z "$bootstrap" ]]; then
    fail "docs/releasing.md does not document the one-time bootstrap publish"
    return 1
  fi
  if [[ -z "$configure" ]]; then
    fail "docs/releasing.md does not document Trusted Publisher configuration"
    return 1
  fi
  if (( configure < bootstrap )); then
    fail "docs/releasing.md configures a Trusted Publisher before the bootstrap publish"
    return 1
  fi
  if ! grep -q 'check-npm-sequence.sh' "$docs"; then
    fail "docs/releasing.md does not point at check-npm-sequence.sh"
    return 1
  fi
  ok "runbook order: bootstrap publish before Trusted Publisher configuration"
}

run_checks() {
  local root="$1" rc=0
  check_publisher_order "$root" || rc=1
  check_optional_deps "$root" || rc=1
  check_platform_map "$root" || rc=1
  check_docs_order "$root" || rc=1
  return "$rc"
}

# Writes a healthy npm-release fixture into DIR. Each scenario gets its own
# copy so a mutation cannot leak into the control.
write_fixture() {
  local dir="$1"
  mkdir -p "$dir"/{scripts,docs,integrations/npm/lib}
  cat > "$dir/scripts/publish-npm.sh" <<'SH'
TARGETS=(
    "x86_64-unknown-linux-musl:linux-x64:do-harness-linux-x64:tar.gz:do-harness"
)
for entry in "${TARGETS[@]}"; do
    :
done
publish_dir "$stage" "do-harness"
SH
  printf '{"optionalDependencies": {"do-harness-linux-x64": "0.1.1"}}\n' \
    > "$dir/integrations/npm/package.json"
  printf '"do-harness-linux-x64"\n' > "$dir/integrations/npm/lib/platform.js"
  printf '# Releasing\nbootstrap publish first\nconfigure its Trusted Publisher\ncheck-npm-sequence.sh\n' \
    > "$dir/docs/releasing.md"
}

# Mutation controls: each perturbation must make its check fail, otherwise the
# check is decorative. Fixtures are synthesized under a temp dir so nothing
# outside it is touched.
self_test() {
  tmp="$(mktemp -d)"
  # `tmp` is deliberately global: the EXIT trap runs after this function
  # returns, and a `local` would already be out of scope there.
  trap 'rm -rf "$tmp"' EXIT
  local rc=0

  write_fixture "$tmp/good"
  if run_checks "$tmp/good" >/dev/null 2>&1; then
    printf 'good: OK\n'
  else
    printf 'good: FAIL: healthy fixture was rejected\n'
    rc=1
  fi

  # Mutation 1: move the meta publish step above the platform loop.
  write_fixture "$tmp/order"
  cat > "$tmp/order/scripts/publish-npm.sh" <<'SH'
TARGETS=(
    "x86_64-unknown-linux-musl:linux-x64:do-harness-linux-x64:tar.gz:do-harness"
)
publish_dir "$stage" "do-harness"
for entry in "${TARGETS[@]}"; do
    :
done
SH
  if check_publisher_order "$tmp/order" >/dev/null 2>&1; then
    printf 'bad-publisher-order: FAIL: meta-before-platform was accepted\n'
    rc=1
  else
    printf 'bad-publisher-order: OK: meta-before-platform rejected\n'
  fi

  # Mutation 2: swap the two documented runbook steps.
  write_fixture "$tmp/docs-order"
  printf '# Releasing\nconfigure its Trusted Publisher\nbootstrap publish first\ncheck-npm-sequence.sh\n' \
    > "$tmp/docs-order/docs/releasing.md"
  if check_docs_order "$tmp/docs-order" >/dev/null 2>&1; then
    printf 'bad-docs-order: FAIL: configure-before-bootstrap was accepted\n'
    rc=1
  else
    printf 'bad-docs-order: OK: configure-before-bootstrap rejected\n'
  fi

  # Mutation 3: drop a platform package from the meta pins.
  write_fixture "$tmp/pin-miss"
  printf '{"optionalDependencies": {}}\n' \
    > "$tmp/pin-miss/integrations/npm/package.json"
  if check_optional_deps "$tmp/pin-miss" >/dev/null 2>&1; then
    printf 'bad-optional-deps: FAIL: an unpinned platform package was accepted\n'
    rc=1
  else
    printf 'bad-optional-deps: OK: an unpinned platform package is rejected\n'
  fi

  # Mutation 4: drop a platform package from the runtime map.
  write_fixture "$tmp/map-miss"
  printf '// empty\n' > "$tmp/map-miss/integrations/npm/lib/platform.js"
  if check_platform_map "$tmp/map-miss" >/dev/null 2>&1; then
    printf 'bad-platform-map: FAIL: an unresolved platform package was accepted\n'
    rc=1
  else
    printf 'bad-platform-map: OK: an unresolved platform package is rejected\n'
  fi

  mkdir -p "$tmp/empty"
  if run_checks "$tmp/empty" >/dev/null 2>&1; then
    printf 'bad-empty: FAIL: an empty workspace was accepted\n'
    rc=1
  else
    printf 'bad-empty: OK: an empty workspace is rejected\n'
  fi

  return "$rc"
}

main() {
  local root="" self=0
  while [[ $# -gt 0 ]]; do
    case "$1" in
      --root)
        [[ $# -ge 2 ]] || { usage >&2; exit 2; }
        root="$2"
        shift 2
        ;;
      --self-test)
        self=1
        shift
        ;;
      -h | --help)
        usage
        exit 0
        ;;
      *)
        printf 'unknown argument: %s\n' "$1" >&2
        usage >&2
        exit 2
        ;;
    esac
  done

  if (( self )); then
    self_test
    exit $?
  fi
  [[ -n "$root" ]] || { usage >&2; exit 2; }
  [[ -d "$root" ]] || { printf 'FAIL: not a directory: %s\n' "$root" >&2; exit 1; }
  run_checks "$root"
}

main "$@"
