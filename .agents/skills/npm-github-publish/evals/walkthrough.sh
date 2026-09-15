#!/usr/bin/env bash
# npm-github-publish walkthrough: leave hermetic evidence for OIDC release guidance.
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

test -s "$root/npm-publish-checklist.md"
test -s "$root/npm-publish-script.txt"
test -s "$root/npm-publish-context.txt"
test -s "$root/npm-publish-negative.txt"
test ! -e "$root/npm_publish_action.txt"
