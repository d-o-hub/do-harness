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
# optionalDependencies already exist on the registry. The meta publish is
# unconditional — gating it on UNAVAILABLE_PKGS is what made `npx do-harness`
# 404 for every Linux/macOS user.
check_publisher_order() {
  local root="$1" publisher="$1/scripts/publish-npm.sh"
  [[ -f "$publisher" ]] || { fail "missing $publisher"; return 1; }
  local loop meta
  # Anchors are deliberately `$`-free: shellcheck SC2016 rejects quoting a
  # literal `$` in grep patterns.
  loop="$(grep -nF -m1 'for entry in ' "$publisher" | cut -d: -f1)"
  meta="$(grep -nF -m1 '"do-harness"' "$publisher" | grep -F 'publish_dir' | cut -d: -f1 | tail -1)"
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

# An unavailable platform package must be skipped explicitly, and the meta
# package must still be published: withholding it breaks `npx do-harness`,
# which is the documented primary install path. The list is the publisher's
# counterpart to the shim's UNAVAILABLE_PACKAGES, and both must agree.
check_unavailable_handling() {
  local root="$1" publisher="$1/scripts/publish-npm.sh"
  [[ -f "$publisher" ]] || { fail "missing $publisher"; return 1; }
  if ! grep -qF 'UNAVAILABLE_PKGS=(' "$publisher"; then
    fail "publisher declares no UNAVAILABLE_PKGS list"
    return 1
  fi
  if ! grep -qF 'is_unavailable ' "$publisher"; then
    fail "publisher does not skip unavailable packages in the target loop"
    return 1
  fi
  # The meta publish must not be gated on the unavailable list: that is what
  # made `npx do-harness` 404 for every Linux/macOS user.
  if ! grep -qE '^publish_dir ' "$publisher"; then
    fail "publisher does not publish the meta package unconditionally"
    return 1
  fi
  # Both consumers must agree, or the shim would offer a package the publisher
  # never uploads (or vice versa).
  local map="$root/integrations/npm/lib/platform.js" pkg
  while IFS= read -r pkg; do
    [[ -n "$pkg" ]] || continue
    if ! grep -qF "\"$pkg\"" "$map"; then
      fail "$pkg is unavailable in the publisher but not in platform.js"
      return 1
    fi
  done < <(sed -n '/^UNAVAILABLE_PKGS=(/,/^)/p' "$publisher" \
    | sed -n 's/^[[:space:]]*"\([^"]*\)".*$/\1/p')
  ok "publisher skips unavailable packages and still publishes the meta package"
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

# Each platform manifest's own `name` must equal the package name in the
# publisher's TARGETS table. A partial rename that touches only the manifest
# would otherwise publish a package the meta pins and the shim maps never
# reference -- the "rename one reference" failure, which every other check
# still passes because they read the publisher, not the manifest.
check_manifest_names() {
  local root="$1" publisher="$1/scripts/publish-npm.sh" bad=0
  [[ -f "$publisher" ]] || { fail "missing $publisher"; return 1; }
  local entry dir pkg expected actual
  while IFS= read -r entry; do
    [[ -n "$entry" ]] || continue
    dir="$(printf '%s' "$entry" | cut -d: -f2)"
    pkg="$(printf '%s' "$entry" | cut -d: -f3)"
    local manifest="$root/integrations/npm/platforms/$dir/package.json"
    if [[ ! -f "$manifest" ]]; then
      fail "platform manifest missing for $dir ($manifest)"
      bad=1
      continue
    fi
    expected="$pkg"
    actual="$(sed -n 's/^[[:space:]]*"name":[[:space:]]*"\([^"]*\)".*/\1/p' "$manifest" | head -1)"
    if [[ "$actual" != "$expected" ]]; then
      fail "platforms/$dir/package.json name '$actual' != publisher package '$expected'"
      bad=1
    fi
  done < <(sed -n '/^TARGETS=(/,/^)/p' "$publisher" \
    | sed -n 's/^[[:space:]]*"\(.*\)".*$/\1/p')
  (( bad == 0 )) || return 1
  ok "platform manifests match the publisher package names"
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

# The publish job must authenticate with OIDC alone. The v0.1.1 npm-publish job
# injected `NODE_AUTH_TOKEN: secrets.NPM_TOKEN`; because the npm CLI treats a
# token as a fallback that only engages when OIDC does not, that job could not
# use trusted publishing and failed with `EOTP` — an error that reads like a 2FA
# problem rather than the credential misconfiguration it was. A token in a job
# that also holds `id-token: write` is always a defect: it either masks the
# trusted-publisher path or silently becomes the real credential.
check_publish_workflow_auth() {
  local root="$1"
  local wf="$root/.github/workflows/release.yml"
  # A workspace without the release workflow has no publish job to validate.
  # Eval sandboxes mirror only the paths a skill names, so the workflow is
  # absent there; skipping keeps the guard usable in both, and the mutation
  # controls below still prove the check can fail.
  if [[ ! -f "$wf" ]]; then
    ok "no .github/workflows/release.yml in this workspace; publish-job auth not checked"
    return 0
  fi
  # Locate the npm-publish job body: from its key to the next 2-space job key.
  local body
  body="$(awk '/^  npm-publish:/{f=1} f&&/^  [a-z-]+:$/&&!/^  npm-publish:/{exit} f' "$wf")"
  if [[ -z "$body" ]]; then
    fail "release.yml has no npm-publish job"
    return 1
  fi
  if ! grep -q 'id-token: write' <<<"$body"; then
    fail "npm-publish job lacks id-token: write"
    return 1
  fi
  local bad
  bad="$(grep -nE 'NODE_AUTH_TOKEN|NPM_TOKEN' <<<"$body" || true)"
  if [[ -n "$bad" ]]; then
    fail "npm-publish injects a publish token ($(tr '\n' ' ' <<<"$bad")) — OIDC alone must authenticate it"
    return 1
  fi
  if ! grep -q 'package-manager-cache: false' <<<"$body"; then
    fail "npm-publish enables npm caching; a poisoned cache is executable in a privileged job"
    return 1
  fi
  check_publish_script_oidc "$root" || return 1
  ok "npm-publish authenticates with OIDC only and disables npm caching"
}

# `actions/setup-node` with `registry-url` unconditionally exports
# `NODE_AUTH_TOKEN=XXXXX-XXXXX-XXXXX-XXXXX` (a placeholder, not a credential)
# and writes it into a temp `.npmrc` as `_authToken=${NODE_AUTH_TOKEN}`. A
# publisher that reads `NODE_AUTH_TOKEN` first therefore sees a non-empty value
# in CI, takes its token branch, and never reaches the OIDC branch — while also
# sending that literal dummy string to the registry as Bearer auth. The
# workflow-level check above cannot see this: the token is generated by the
# action, not written in the YAML. The publisher must recognise and clear it.
check_publish_script_oidc() {
  local root="$1"
  local script="$root/scripts/publish-npm.sh"
  if [[ ! -f "$script" ]]; then
    fail "scripts/publish-npm.sh is missing; cannot verify it reaches the OIDC branch"
    return 1
  fi
  if ! grep -q 'XXXXX-XXXXX-XXXXX-XXXXX' "$script"; then
    fail "publish-npm.sh does not recognise the setup-node placeholder NODE_AUTH_TOKEN; the OIDC branch would be unreachable in CI"
    return 1
  fi
  # Clearing it matters as much as detecting it: the action's temp `.npmrc`
  # interpolates the variable, so leaving the literal in place still sends it.
  if ! grep -qE 'export NODE_AUTH_TOKEN=""' "$script"; then
    fail "publish-npm.sh detects the placeholder but never clears NODE_AUTH_TOKEN; the temp .npmrc would still send the dummy as Bearer auth"
    return 1
  fi
  ok "publish-npm.sh ignores the setup-node placeholder so the OIDC branch stays reachable"
}

run_checks() {
  local root="$1" rc=0
  check_publisher_order "$root" || rc=1
  check_unavailable_handling "$root" || rc=1
  check_manifest_names "$root" || rc=1
  check_optional_deps "$root" || rc=1
  check_platform_map "$root" || rc=1
  check_docs_order "$root" || rc=1
  check_publish_workflow_auth "$root" || rc=1
  return "$rc"
}

# Writes a healthy npm-release fixture into DIR. Each scenario gets its own
# copy so a mutation cannot leak into the control.
write_fixture() {
  local dir="$1"
  mkdir -p "$dir"/{scripts,docs,integrations/npm/lib,integrations/npm/platforms/linux-x64}
  cat > "$dir/scripts/publish-npm.sh" <<'SH'
NODE_AUTH_TOKEN_PLACEHOLDER="XXXXX-XXXXX-XXXXX-XXXXX"
TARGETS=(
    "x86_64-unknown-linux-musl:linux-x64:do-harness-linux-x64:tar.gz:do-harness"
)
UNAVAILABLE_PKGS=(
    "do-harness-win32-x64"
)
if [[ "${NODE_AUTH_TOKEN:-}" == "$NODE_AUTH_TOKEN_PLACEHOLDER" ]]; then
    export NODE_AUTH_TOKEN=""
fi
for entry in "${TARGETS[@]}"; do
    if is_unavailable "$pkg"; then
        continue
    fi
done
publish_dir "$stage" "do-harness"
SH
  printf '{\n  "name": "do-harness-linux-x64",\n  "version": "0.1.1"\n}\n' \
    > "$dir/integrations/npm/platforms/linux-x64/package.json"
  printf '{"optionalDependencies": {"do-harness-linux-x64": "0.1.1"}}\n' \
    > "$dir/integrations/npm/package.json"
  printf '"do-harness-linux-x64"\n' > "$dir/integrations/npm/lib/platform.js"
  printf 'const UNAVAILABLE_PACKAGES = new Set(["do-harness-win32-x64"]);\n' \
    >> "$dir/integrations/npm/lib/platform.js"
  printf '# Releasing\nbootstrap publish first\nconfigure its Trusted Publisher\ncheck-npm-sequence.sh\n' \
    > "$dir/docs/releasing.md"
  mkdir -p "$dir/.github/workflows"
  cat > "$dir/.github/workflows/release.yml" <<'YML'
name: release
jobs:
  npm-publish:
    permissions:
      contents: read
      id-token: write
    steps:
      - uses: actions/setup-node@0000000000000000000000000000000000000000
        with:
          node-version: 24
          registry-url: https://registry.npmjs.org
          package-manager-cache: false
      - run: bash scripts/publish-npm.sh --dist dist
  other:
    steps:
      - run: true
YML
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

  # Mutation 3: drop a platform package from the runtime map.
  write_fixture "$tmp/map-miss"
  printf '// empty\n' > "$tmp/map-miss/integrations/npm/lib/platform.js"
  if check_platform_map "$tmp/map-miss" >/dev/null 2>&1; then
    printf 'bad-platform-map: FAIL: an unresolved platform package was accepted\n'
    rc=1
  else
    printf 'bad-platform-map: OK: an unresolved platform package is rejected\n'
  fi

  # Mutation 5: rename ONLY the platform manifest. Publisher, meta pins, and
  # shim map stay consistent, so only the name check can catch it.
  write_fixture "$tmp/manifest-rename"
  printf '{\n  "name": "do-harness-windows-x64",\n  "version": "0.1.1"\n}\n' \
    > "$tmp/manifest-rename/integrations/npm/platforms/linux-x64/package.json"
  if check_manifest_names "$tmp/manifest-rename" >/dev/null 2>&1; then
    printf 'bad-manifest-name: FAIL: a manifest-only rename was accepted\n'
    rc=1
  else
    printf 'bad-manifest-name: OK: a manifest-only rename is rejected\n'
  fi

  # Mutation 6: re-gating the meta publish on the unavailable list must be
  # rejected — that gate is exactly what made `npx do-harness` 404 for every
  # Linux/macOS user.
  write_fixture "$tmp/withheld-meta"
  python3 - "$tmp/withheld-meta/scripts/publish-npm.sh" <<'PY'
import sys, pathlib
p = pathlib.Path(sys.argv[1])
s = p.read_text()
s = (s.replace('publish_dir "$stage" "do-harness"',
               'if (( ${#UNAVAILABLE_PKGS[@]} > 0 )); then\n'
               '    echo "meta withheld"\n'
               'else\n'
               '    publish_dir "$stage" "do-harness"\n'
               'fi')
     .replace('^publish_dir ', 'publish_dir '))
p.write_text(s)
PY
  if check_unavailable_handling "$tmp/withheld-meta" >/dev/null 2>&1; then
    printf 'bad-withheld-meta: FAIL: withholding the meta package was accepted\n'
    rc=1
  else
    printf 'bad-withheld-meta: OK: withholding the meta package is rejected\n'
  fi

  # Mutation 7: the shim and the publisher must agree, or one offers a package
  # the other never uploads.
  write_fixture "$tmp/disagree"
  printf '"do-harness-linux-x64"\n' > "$tmp/disagree/integrations/npm/lib/platform.js"
  if check_unavailable_handling "$tmp/disagree" >/dev/null 2>&1; then
    printf 'bad-consumer-disagreement: FAIL: publisher/shim disagreement accepted\n'
    rc=1
  else
    printf 'bad-consumer-disagreement: OK: publisher/shim disagreement rejected\n'
  fi

  # Mutation 8: dropping the skip from the loop must be rejected.
  write_fixture "$tmp/no-skip"
  python3 - "$tmp/no-skip/scripts/publish-npm.sh" <<'PY'
import sys, pathlib
p = pathlib.Path(sys.argv[1])
s = p.read_text()
s = s.replace('    if is_unavailable "$pkg"; then\n        continue\n    fi\n', "")
p.write_text(s)
PY
  if check_unavailable_handling "$tmp/no-skip" >/dev/null 2>&1; then
    printf 'bad-missing-skip: FAIL: a missing loop skip was accepted\n'
    rc=1
  else
    printf 'bad-missing-skip: OK: a missing loop skip is rejected\n'
  fi

  # Mutation 9: a publish token in an OIDC job must be rejected. This is the
  # v0.1.1 defect: the injected token forced the npm CLI's fallback path and the
  # job failed with EOTP instead of publishing via trusted publishing.
  write_fixture "$tmp/token-in-oidc"
  python3 - "$tmp/token-in-oidc/.github/workflows/release.yml" <<'PY'
import sys, pathlib
p = pathlib.Path(sys.argv[1])
s = p.read_text()
s = s.replace('  npm-publish:\n',
              '  npm-publish:\n    env:\n      NODE_AUTH_TOKEN: ${{ secrets.NPM_TOKEN }}\n')
p.write_text(s)
PY
  if check_publish_workflow_auth "$tmp/token-in-oidc" >/dev/null 2>&1; then
    printf 'bad-token-in-oidc: FAIL: a publish token in an OIDC job was accepted\n'
    rc=1
  else
    printf 'bad-token-in-oidc: OK: a publish token in an OIDC job is rejected\n'
  fi

  # Mutation 10: npm caching in the privileged publish job must be rejected.
  write_fixture "$tmp/cached-publish"
  python3 - "$tmp/cached-publish/.github/workflows/release.yml" <<'PY'
import sys, pathlib
p = pathlib.Path(sys.argv[1])
s = p.read_text()
s = s.replace('          package-manager-cache: false\n', '')
p.write_text(s)
PY
  if check_publish_workflow_auth "$tmp/cached-publish" >/dev/null 2>&1; then
    printf 'bad-cached-publish: FAIL: npm caching in the publish job was accepted\n'
    rc=1
  else
    printf 'bad-cached-publish: OK: npm caching in the publish job is rejected\n'
  fi

  # Mutation 11: dropping OIDC permission must be rejected.
  write_fixture "$tmp/no-id-token"
  python3 - "$tmp/no-id-token/.github/workflows/release.yml" <<'PY'
import sys, pathlib
p = pathlib.Path(sys.argv[1])
s = p.read_text()
s = s.replace('      id-token: write\n', '')
p.write_text(s)
PY
  if check_publish_workflow_auth "$tmp/no-id-token" >/dev/null 2>&1; then
    printf 'bad-no-id-token: FAIL: a publish job without id-token was accepted\n'
    rc=1
  else
    printf 'bad-no-id-token: OK: a publish job without id-token is rejected\n'
  fi

  mkdir -p "$tmp/empty"
  if run_checks "$tmp/empty" >/dev/null 2>&1; then
    printf 'bad-empty: FAIL: an empty workspace was accepted\n'
    rc=1
  else
    printf 'bad-empty: OK: an empty workspace is rejected\n'
  fi

  # Mutation 12: a publisher that does not recognise the setup-node placeholder
  # must be rejected. This is the real defect: setup-node exports
  # `NODE_AUTH_TOKEN=XXXXX-...`, the publisher's token branch won on
  # non-emptiness, and the OIDC branch never ran.
  write_fixture "$tmp/placeholder-blind"
  python3 - "$tmp/placeholder-blind/scripts/publish-npm.sh" <<'PY'
import sys, pathlib
p = pathlib.Path(sys.argv[1])
s = p.read_text()
s = s.replace('NODE_AUTH_TOKEN_PLACEHOLDER="XXXXX-XXXXX-XXXXX-XXXXX"',
              'NODE_AUTH_TOKEN_PLACEHOLDER="__never_matches__"')
p.write_text(s)
PY
  if check_publish_script_oidc "$tmp/placeholder-blind" >/dev/null 2>&1; then
    printf 'bad-placeholder-blind: FAIL: a publisher blind to the placeholder was accepted\n'
    rc=1
  else
    printf 'bad-placeholder-blind: OK: a publisher blind to the placeholder is rejected\n'
  fi

  # Mutation 13: detecting the placeholder without clearing it must be rejected.
  # The action's temp `.npmrc` interpolates the variable, so leaving the literal
  # in the environment still sends the dummy string as Bearer auth.
  write_fixture "$tmp/placeholder-uncleared"
  python3 - "$tmp/placeholder-uncleared/scripts/publish-npm.sh" <<'PY'
import sys, pathlib
p = pathlib.Path(sys.argv[1])
s = p.read_text()
# Indentation-agnostic: the real script indents the statement, the fixture does
# not, and a whitespace-sensitive replace silently no-ops (turning a mutation
# control into a false pass).
assert 'export NODE_AUTH_TOKEN=""' in s, "mutation target missing from fixture"
s = s.replace('export NODE_AUTH_TOKEN=""', 'export NODE_AUTH_TOKEN="${NODE_AUTH_TOKEN_PLACEHOLDER}"')
p.write_text(s)
PY
  if check_publish_script_oidc "$tmp/placeholder-uncleared" >/dev/null 2>&1; then
    printf 'bad-placeholder-uncleared: FAIL: an uncleared placeholder was accepted\n'
    rc=1
  else
    printf 'bad-placeholder-uncleared: OK: an uncleared placeholder is rejected\n'
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
