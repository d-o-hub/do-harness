#!/usr/bin/env bash
# tokio-performance walkthrough: extracts the offload decision table and the
# review flags from the skill into a playbook artifact, and compiles + runs a
# std-only demo of the lock-scope fix the skill prescribes (flag 3).
set -euo pipefail
root="${DO_HARNESS_ROOT:?DO_HARNESS_ROOT required}"
skill="$root/.agents/skills/tokio-performance/SKILL.md"

playbook="$root/tokio_playbook.md"
{
    echo "tokio offload playbook, extracted from .agents/skills/tokio-performance/SKILL.md"
    sed -n '/^## Decision Table/,/^## Async Code Review Flags/p' "$skill" |
        grep -E '^\| \*\*'
    sed -n '/^## Async Code Review Flags/,$p' "$skill" |
        grep -E '^[0-9]+\. \*\*'
} > "$playbook"
test -s "$playbook"

# The lock-scope fix (flag 3) must compile and run: the guard is dropped before
# the boundary instead of being held across it.
cat > "$root/lock_scope.rs" << 'RUST'
use std::sync::{Arc, Mutex};

fn scoped_update(state: &Arc<Mutex<Vec<u32>>>) -> u32 {
    let len = {
        let guard = state.lock().expect("poisoned");
        guard.len() as u32
    };
    len + 1
}

fn main() {
    let state = Arc::new(Mutex::new(vec![1, 2, 3]));
    assert_eq!(scoped_update(&state), 4);
    println!("lock scope ok");
}
RUST
rustc --edition 2021 -o "$root/lock_scope" "$root/lock_scope.rs"
"$root/lock_scope" > "$root/lock_scope_out.txt"

cat <<'EOF' > "$root/tokio_negative.txt"
out-of-scope: no async or Tokio code is involved
EOF
