#!/usr/bin/env bash
# npm-github-publish walkthrough: leave hermetic evidence for OIDC release guidance.
#
# Two kinds of residue are produced, both inside DO_HARNESS_ROOT:
#   1. guidance artifacts the checklist/negative cases assert on;
#   2. a REAL publisher run against a local stub registry, so the idempotency
#      and publish-order cases grade observable tool output instead of text
#      this script wrote itself.
set -euo pipefail
root="${DO_HARNESS_ROOT:?DO_HARNESS_ROOT required}"

cat > "$root/npm-publish-checklist.md" <<'MD'
# npm GitHub trusted-publishing checklist
- Configure a trusted publisher for every package with the exact organization, repository, and release.yml workflow filename.
- Grant the publish job `id-token: write` and `contents: read`; use no NPM_TOKEN or NODE_AUTH_TOKEN write secret.
- Require Node >= 22.14.0 and npm >= 11.5.1 before publishing.
- Allow direct npm publish when the workflow uses `npm publish`; stage-only is a different contract.
- npm OIDC publish uses short-lived credentials and public GitHub Actions publishes receive provenance automatically.
MD

cat > "$root/npm-publish-script.txt" <<'MD'
- npm whoami is not an OIDC probe; trust the GitHub Actions OIDC environment only for the publish operation.
- Use an anonymous registry metadata GET for an exact package/version idempotency check.
- The exact package/version lookup must skip only a version that already exists.
MD

cat > "$root/npm-publish-context.txt" <<'MD'
- Build and verify all release artifacts first.
- Publish platform packages first and the meta package last.
- Keep npm publish --dry-run for hermetic assembly verification.
- OIDC or artifact failures fail closed; never guess a publish token.
MD

cat > "$root/npm-publish-negative.txt" <<'MD'
out-of-scope: unrelated prose does not trigger a package release workflow
MD

# --- real publisher run against a local stub registry ---------------------
# The stub answers package/version probes for the four "already published"
# platform packages with 200 and everything else with 404, which is exactly the
# partial-publication state that motivates the registry-name cases.

stub_root="$root/npm-stub"
mkdir -p "$stub_root/registry" "$stub_root/dist" "$stub_root/pkg"
for pkg in do-harness-linux-x64 do-harness-linux-arm64 \
    do-harness-darwin-x64 do-harness-darwin-arm64; do
    mkdir -p "$stub_root/registry/$pkg"
    printf '{"name":"%s","version":"0.1.1"}\n' "$pkg" > "$stub_root/registry/$pkg/0.1.1"
done

cat > "$stub_root/stub.py" <<'PY'
import http.server
import socketserver
import sys

root = sys.argv[1]


class Handler(http.server.SimpleHTTPRequestHandler):
    def __init__(self, *args, **kwargs):
        super().__init__(*args, directory=root, **kwargs)

    def log_message(self, *args):
        pass


with socketserver.TCPServer(("127.0.0.1", 0), Handler) as httpd:
    print(httpd.server_address[1], flush=True)
    httpd.serve_forever()
PY

python3 "$stub_root/stub.py" "$stub_root/registry" > "$stub_root/port" 2>/dev/null &
stub_pid=$!
trap 'kill "$stub_pid" 2>/dev/null || true' EXIT
for _ in $(seq 1 100); do
    [[ -s "$stub_root/port" ]] && break
    sleep 0.1
done
[[ -s "$stub_root/port" ]] || { echo "stub registry did not start" >&2; exit 1; }
stub_port="$(cat "$stub_root/port")"

# Release-shaped fixtures: the publisher extracts these by target name.
version="0.1.1"
for target in x86_64-unknown-linux-musl aarch64-unknown-linux-musl \
    x86_64-apple-darwin aarch64-apple-darwin; do
    name="do-harness-v${version}-${target}"
    mkdir -p "$stub_root/pkg/$name"
    printf 'stub binary\n' > "$stub_root/pkg/$name/do-harness"
    tar -czf "$stub_root/dist/${name}.tar.gz" -C "$stub_root/pkg" "$name"
done
win="do-harness-v${version}-x86_64-pc-windows-msvc"
mkdir -p "$stub_root/pkg/$win"
printf 'stub binary\n' > "$stub_root/pkg/$win/do-harness.exe"
python3 - "$stub_root/pkg/$win" "$stub_root/dist/${win}.zip" <<'PY'
import os
import sys
import zipfile

src, out = sys.argv[1], sys.argv[2]
with zipfile.ZipFile(out, "w") as archive:
    for base, _dirs, files in os.walk(src):
        for name in files:
            path = os.path.join(base, name)
            archive.write(path, os.path.relpath(path, os.path.dirname(src)))
PY

NPM_CONFIG_REGISTRY="http://127.0.0.1:${stub_port}" \
    bash "$root/scripts/publish-npm.sh" \
    --dist "$stub_root/dist" --version "$version" --dry-run \
    > "$root/npm-publish-run.txt" 2>&1

kill "$stub_pid" 2>/dev/null || true
trap - EXIT

# --- executable sequence guard, run as part of the skill's own evidence ----
# The bootstrap -> configure -> OIDC order is enforced by the script below, so
# this walkthrough proves the guard runs, passes on the mirrored tree, and
# still fails on its mutation controls. Counting lines from this file would
# grade nothing; these are the guard's own exit codes and output.
guard="$root/.agents/skills/npm-github-publish/scripts/check-npm-sequence.sh"
if [[ -x "$guard" ]]; then
    {
        bash "$guard" --root "$root"
        echo "guard-root-exit=$?"
        bash "$guard" --self-test
        echo "guard-self-test-exit=$?"
    } > "$root/npm-sequence-guard.txt" 2>&1
    # A failing guard must fail the walkthrough, not just the assertion, so the
    # skill cannot claim evidence from a broken check.
    grep -q 'guard-root-exit=0' "$root/npm-sequence-guard.txt"
    grep -q 'guard-self-test-exit=0' "$root/npm-sequence-guard.txt"
fi

test -s "$root/npm-publish-checklist.md"
test -s "$root/npm-publish-script.txt"
test -s "$root/npm-publish-context.txt"
test -s "$root/npm-publish-run.txt"
test -s "$root/npm-publish-negative.txt"
test ! -e "$root/npm_publish_action.txt"
