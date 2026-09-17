#!/usr/bin/env bash
# harness walkthrough: proves greenfield adoption end-to-end with the real
# binary — init scaffolds the workspace plus a minimal crate, the full rust
# sensor suite must actually pass (no assumed green), then a broken workspace
# drives the fail-fast error-signature lifecycle (record -> list) as residue.
set -euo pipefail
root="${DO_HARNESS_ROOT:?DO_HARNESS_ROOT required}"
bin="${DO_HARNESS_BIN:-do-harness}"

# Greenfield adoption: the full suite (fmt/check/clippy/test/loc/commitlint)
# runs against the scaffolded crate and must exit 0.
#
# `--minimal` is load-bearing, not cosmetic: without it `init` re-scaffolds
# `.agents/skills/harness/SKILL.md` + `references/`, which is the guidance the
# without-skill baseline strips. The baseline would then grade a regenerated
# copy of the guidance it was supposed to measure without, and Skill Lift is
# reported as contaminated instead of measured. Skill scaffolding is already
# covered by the repository's own skills-sync sensor, so this walkthrough only
# needs the workspace + crate. Keep this comment free of bare tree tokens: the
# eval sandbox mirrors any repo path named here, and a mirrored non-hidden
# entry would make `init` detect the generic pack instead of greenfield Rust.
"$bin" --root "$root" init --minimal >/dev/null
"$bin" --root "$root" verify

# Break the crate: the test sensor must fail and record its signature.
rm -rf "$root/Cargo.toml" "$root/src"
"$bin" --root "$root" verify --only test --record >/dev/null 2>&1 || true

"$bin" --root "$root" errors list

# Self-correction: minimal fix (restore the crate), then prove green again.
"$bin" --root "$root" init --minimal >/dev/null
"$bin" --root "$root" verify

# Negative case: a general-knowledge question needs no workspace at all, so
# init scaffolds nothing and no crate residue is attributable to it.
cat > "$root/harness_negative.txt" << 'TXT'
negative: general-knowledge question needs no init, no workspace touched
TXT
