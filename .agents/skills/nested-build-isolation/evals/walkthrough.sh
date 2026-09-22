#!/usr/bin/env bash
# nested-build-isolation walkthrough: proves the documented strip list actually
# removes the launcher's state from a nested spawn, on a fixture child only.
# No repository paths are named here on purpose: the eval sandbox mirrors every
# repo path a walkthrough mentions, and this skill needs none.
set -euo pipefail
root="${DO_HARNESS_ROOT:?DO_HARNESS_ROOT required}"

child="$root/child.sh"
cat > "$child" <<'EOF'
#!/usr/bin/env bash
env | sort
EOF
chmod +x "$child"

# One parent environment, two spawn policies: the leak lives in the parent, the
# strip list is what the sanitized arm applies.
export RUSTC_WRAPPER=cargo-llvm-cov
export CARGO_TARGET_DIR="$root/target/llvm-cov-target"
export RUSTFLAGS="-C instrument-coverage"
export LLVM_PROFILE_FILE="$root/cov-%p.profraw"

"$child" > "$root/isolation-raw.env"

# Sanitized spawn: the strip list from SKILL.md, profile redirected to /dev/null.
env -u RUSTC_WRAPPER -u CARGO_BUILD_RUSTC_WRAPPER -u RUSTC -u RUSTDOC \
    -u RUSTFLAGS -u RUSTDOCFLAGS -u CARGO_TARGET_DIR -u CARGO_INCREMENTAL \
    -u CARGO_ENCODED_RUSTFLAGS -u CARGO_LLVM_COV -u CARGO_LLVM_COV_TARGET_DIR \
    LLVM_PROFILE_FILE=/dev/null \
    "$child" > "$root/isolation-sanitized.env"

# What survived each arm, as one line per key so the assertions stay simple.
{
    for key in RUSTC_WRAPPER CARGO_TARGET_DIR LLVM_PROFILE_FILE; do
        raw="absent"
        grep -q "^${key}=" "$root/isolation-raw.env" && raw="present"
        sanitized="absent"
        grep -q "^${key}=" "$root/isolation-sanitized.env" && sanitized="present"
        printf '%s raw=%s sanitized=%s\n' "$key" "$raw" "$sanitized"
    done
} > "$root/isolation-keys.txt"

printf 'profraw files under the sandbox: %s\n' \
    "$(find "$root" -name '*.profraw' | wc -l | tr -d ' ')" > "$root/isolation-litter.txt"

# Out-of-scope control: this fixture builds nothing and runs no instrumented
# session, so the skill stays unloaded and no sensor is ever disabled.
printf 'out-of-scope: no nested build and no instrumented session in this fixture\n' \
    > "$root/isolation_negative.txt"
