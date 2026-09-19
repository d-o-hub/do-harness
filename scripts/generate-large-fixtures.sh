#!/usr/bin/env bash
# generate-large-fixtures.sh — regenerate the realistic large-diff routing corpus.
#
# Writes `tests/fixtures/pr-routing-large/` from scratch: the 12-class manifest,
# one `base/`+`head/` tree pair per class, and a copy of the shared fake
# provider. Every byte is emitted from a fixed loop, so two runs are identical
# and the corpus stays reviewable in git.
#
# The minimal corpus (`tests/fixtures/pr-routing/`) proves the route *decision*:
# every case is a few hundred bytes, which is smaller than the router envelope.
# This corpus measures the route *economy* on diffs whose size is representative
# of a real pull request.
#
# Usage: generate-large-fixtures.sh [OUTPUT_DIR]
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"
OUTPUT="${1:-$ROOT/tests/fixtures/pr-routing-large}"
SOURCE_CORPUS="$ROOT/tests/fixtures/pr-routing"

# Resolves a path to an absolute physical path without requiring it to exist
# yet, and drops any trailing slash. Textual comparison alone is not enough:
# `tests/fixtures/../fixtures` and a tab-completed `tests/fixtures/` both name
# the same directory as the plain spelling.
normalize() {  # <path>
  local p="$1" tail="" base resolved
  while [[ ! -e "$p" ]]; do
    tail="/$(basename "$p")$tail"
    p="$(dirname "$p")"
    [[ "$p" == "/" || "$p" == "." ]] && break
  done
  base="$(cd "$p" 2>/dev/null && pwd -P)" || base="$p"
  # Never let the strip turn the filesystem root into an empty string: an empty
  # OUTPUT would defeat the root guard below.
  if [[ "$base" == "/" ]]; then
    resolved="/"
  else
    resolved="${base%/}$tail"
    resolved="${resolved%/}"
  fi
  printf '%s' "$resolved"
}

OUTPUT="$(normalize "$OUTPUT")"
SOURCE_CORPUS="$(normalize "$SOURCE_CORPUS")"

# The script deletes OUTPUT before regenerating, so a typo must never point it
# at the working tree or at the corpus it reads. An ancestor-or-self of either
# would take the repository (or the source corpus) down with the target.
refuse() {  # <reason>
  printf 'generate-large-fixtures: refusing to write to %s (%s)\n' "$OUTPUT" "$1" >&2
  exit 2
}

if [[ -z "$OUTPUT" || "$OUTPUT" == "/" ]]; then
  refuse "filesystem root"
fi
case "$ROOT/" in
  "$OUTPUT"/*) refuse "it contains the repository root" ;;
  *) ;;
esac
case "$SOURCE_CORPUS/" in
  "$OUTPUT"/*) refuse "it contains the source corpus" ;;
  *) ;;
esac

if [[ ! -f "$SOURCE_CORPUS/manifest.json" || ! -f "$SOURCE_CORPUS/fake-router.sh" ]]; then
  printf 'generate-large-fixtures: source corpus missing under %s\n' "$SOURCE_CORPUS" >&2
  exit 2
fi

rm -rf "$OUTPUT"
mkdir -p "$OUTPUT"

# Creates one case directory, with an empty `base/` and `head/` plus any
# subdirectories the case's files live in.
begin_case() {  # <case> [subdir...]
  local case_name="$1"
  shift
  local dir="$OUTPUT/$case_name" sub
  mkdir -p "$dir/base" "$dir/head"
  for sub in "$@"; do
    mkdir -p "$dir/base/$sub" "$dir/head/$sub"
  done
}

# Writes one side of a rename case. The ident appears in a use, a struct, an
# impl, and one call site per function, so a rename touches every line class
# the router's mechanical-change reasoning cares about.
write_rename_side() {  # <dest> <file-stem> <ident> <sites>
  local dest="$1" stem="$2" ident="$3" sites="$4" i
  {
    printf 'use crate::%s;\n\n' "$ident"
    printf 'pub struct %s {\n    inner: u64,\n}\n\n' "$ident"
    printf 'impl %s {\n    pub fn new(inner: u64) -> Self {\n        Self { inner }\n    }\n}\n\n' "$ident"
    for i in $(seq 1 "$sites"); do
      printf 'pub fn %s_%02d(value: %s) -> %s {\n    let mapped = %s::from(value);\n    mapped.normalize()\n}\n\n' \
        "$stem" "$i" "$ident" "$ident" "$ident"
    done
  } > "$dest"
}

emit_rename_tree() {  # <case> <old-ident> <new-ident> <sites-per-file>
  local case_name="$1" old="$2" new="$3" sites="$4"
  begin_case "$case_name" src
  local dir="$OUTPUT/$case_name" file
  for file in alpha beta gamma delta epsilon; do
    write_rename_side "$dir/base/src/$file.rs" "$file" "$old" "$sites"
    write_rename_side "$dir/head/src/$file.rs" "$file" "$new" "$sites"
  done
}

# A module whose error enum and Display match arms grow together, so the added
# variants are always covered by a reachable failure path.
write_error_module() {  # <dest> <module> <variant-count>
  local dest="$1" module="$2" count="$3" i
  {
    printf 'use std::fmt;\n\n'
    printf '#[derive(Debug)]\n'
    printf 'pub enum %sError {\n' "$module"
    for i in $(seq 1 "$count"); do
      printf '    Variant%02d,\n' "$i"
    done
    printf '}\n\n'
    printf 'impl fmt::Display for %sError {\n' "$module"
    printf "    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {\n"
    printf '        match self {\n'
    for i in $(seq 1 "$count"); do
      printf '            Self::Variant%02d => write!(f, "%s error variant %02d"),\n' \
        "$i" "$module" "$i"
    done
    printf '        }\n'
    printf '    }\n'
    printf '}\n'
  } > "$dest"
}

emit_docs_only() {
  begin_case docs-only
  local dir="$OUTPUT/docs-only" i release
  {
    printf '# Changelog\n\n## 1.0.0\n\n'
    for i in $(seq 1 12); do
      printf -- '- shipped fix %d for the initial release\n' "$i"
    done
  } > "$dir/base/CHANGELOG.md"
  {
    printf '# Changelog\n\n'
    for release in 2 3 4; do
      printf '## 2.%s.0\n\n' "$release"
      for i in $(seq 1 32); do
        printf -- '- documented behaviour change %s.%02d for the migration guide and release notes\n' \
          "$release" "$i"
      done
      printf '\n'
    done
  } > "$dir/head/CHANGELOG.md"
}

emit_tests_only() {
  begin_case tests-only tests
  local dir="$OUTPUT/tests-only" i
  {
    printf 'use crate::CASES;\n\n'
    for i in 1 2; do
      printf '#[test]\nfn case_%02d_is_registered() {\n    let ids: Vec<u32> = CASES.iter().map(|case| case.id).collect();\n    assert!(ids.contains(&%d), "case %d must stay registered");\n}\n\n' \
        "$i" "$i" "$i"
    done
  } > "$dir/base/tests/integration.rs"
  {
    printf 'use crate::CASES;\n\n'
    for i in $(seq 1 20); do
      printf '#[test]\nfn case_%02d_is_registered() {\n    let ids: Vec<u32> = CASES.iter().map(|case| case.id).collect();\n    assert!(ids.contains(&%d), "case %d must stay registered");\n}\n\n' \
        "$i" "$i" "$i"
    done
  } > "$dir/head/tests/integration.rs"
}

emit_lockfile_only() {
  begin_case lockfile-only
  local dir="$OUTPUT/lockfile-only" i
  {
    printf 'version = 3\n\n'
    for i in $(seq 1 50); do
      printf '[[package]]\nname = "crate-%02d"\nversion = "1.0.0"\nsource = "registry+https://github.com/rust-lang/crates.io-index"\nchecksum = "00000000000000000000000000000000000000000000000000000000000000%02d"\n\n' \
        "$i" "$i"
    done
  } > "$dir/base/Cargo.lock"
  {
    printf 'version = 3\n\n'
    for i in $(seq 1 50); do
      printf '[[package]]\nname = "crate-%02d"\nversion = "1.1.0"\nsource = "registry+https://github.com/rust-lang/crates.io-index"\nchecksum = "11111111111111111111111111111111111111111111111111111111111111%02d"\n\n' \
        "$i" "$i"
    done
  } > "$dir/head/Cargo.lock"
}

emit_internal_refactor() {
  emit_rename_tree internal-refactor OldName NewName 7
}

emit_behavior_change() {
  begin_case behavior-change src
  local dir="$OUTPUT/behavior-change" i
  {
    printf '/// Folds the sample buffer into a single accumulator.\n'
    printf 'pub fn evaluate(samples: &[u64]) -> u64 {\n'
    printf '    let mut acc = 0u64;\n'
    for i in $(seq 1 100); do
      printf '    acc = acc.saturating_add(samples.get(%d).copied().unwrap_or_default().wrapping_mul(%d));\n' \
        "$i" "$i"
    done
    printf '    acc\n'
    printf '}\n'
  } > "$dir/base/src/engine.rs"
  {
    printf '/// Folds the sample buffer into a single windowed accumulator.\n'
    printf 'pub fn evaluate(samples: &[u64]) -> u64 {\n'
    printf '    let mut acc = 0u64;\n'
    for i in $(seq 1 100); do
      printf '    let window_%d = samples.get(%d).copied().unwrap_or_default().checked_mul(%d);\n' \
        "$i" "$i" "$i"
      printf '    acc = acc.checked_add(window_%d.unwrap_or(acc %% 97)).unwrap_or(acc);\n' "$i"
    done
    printf '    acc\n'
    printf '}\n'
  } > "$dir/head/src/engine.rs"
}

emit_public_api_break() {
  begin_case public-api-break src
  local dir="$OUTPUT/public-api-break" i
  {
    printf '//! Public surface of the sample crate.\n\n'
    for i in $(seq 1 15); do
      printf '/// Computes the value for slot %02d.\n' "$i"
      printf 'pub fn compute_%02d(\n    value: u64,\n) -> u64 {\n    value.wrapping_mul(%d)\n}\n\n' \
        "$i" "$i"
    done
  } > "$dir/base/src/lib.rs"
  {
    printf '//! Public surface of the sample crate.\n\n'
    for i in $(seq 1 15); do
      printf '/// Computes the scaled value for slot %02d.\n' "$i"
      printf 'pub fn compute_%02d(\n    value: u64,\n    scale: u64,\n) -> u64 {\n    value.wrapping_mul(%d).wrapping_mul(scale)\n}\n\n' \
        "$i" "$i"
    done
  } > "$dir/head/src/lib.rs"
}

emit_security() {
  begin_case security src
  local dir="$OUTPUT/security" i
  {
    printf '//! Credential verification.\n\n'
    printf 'pub struct Credentials {\n    user: String,\n    secret: String,\n}\n\n'
    printf 'pub fn verify(stored: &Credentials, presented: &str) -> bool {\n'
    printf '    stored.secret == presented\n'
    printf '}\n\n'
    printf 'pub fn describe(stored: &Credentials) -> String {\n'
    printf '    format!("{}:{}", stored.user, stored.secret)\n'
    printf '}\n'
  } > "$dir/base/src/auth.rs"
  {
    printf '//! Credential verification with constant-time comparison and rotation.\n\n'
    printf 'use std::time::{Duration, SystemTime};\n\n'
    printf 'pub struct Credentials {\n    user: String,\n    digest: [u8; 32],\n    salt: [u8; 16],\n    rotated_at: SystemTime,\n}\n\n'
    printf 'pub fn verify(stored: &Credentials, presented: &str) -> bool {\n'
    printf '    let candidate = digest_of(presented.as_bytes(), &stored.salt);\n'
    printf '    constant_time_eq(&candidate, &stored.digest)\n'
    printf '}\n\n'
    printf 'pub fn needs_rotation(stored: &Credentials, max_age: Duration) -> bool {\n'
    printf '    match SystemTime::now().duration_since(stored.rotated_at) {\n'
    printf '        Ok(age) => age > max_age,\n'
    printf '        Err(_) => true,\n'
    printf '    }\n'
    printf '}\n\n'
    printf 'fn constant_time_eq(left: &[u8; 32], right: &[u8; 32]) -> bool {\n'
    printf '    let mut diff = 0u8;\n'
    for i in $(seq 0 31); do
      printf '    diff |= left[%d] ^ right[%d];\n' "$i" "$i"
    done
    printf '    diff == 0\n'
    printf '}\n\n'
    printf 'fn digest_of(input: &[u8], salt: &[u8; 16]) -> [u8; 32] {\n'
    printf '    let mut state = [0u8; 32];\n'
    for i in $(seq 0 15); do
      printf '    state[%d] = input.get(%d).copied().unwrap_or(salt[%d]).wrapping_add(%d);\n' \
        "$i" "$i" "$i" "$i"
      printf '    state[%d] = state[%d].rotate_left(%d);\n' \
        "$((i + 16))" "$i" "$((i + 1))"
    done
    printf '    state\n'
    printf '}\n\n'
    printf 'pub fn describe(stored: &Credentials) -> String {\n'
    printf '    format!("{}:rotated", stored.user)\n'
    printf '}\n'
  } > "$dir/head/src/auth.rs"
}

emit_persistence_schema() {
  begin_case persistence-schema src migrations
  local dir="$OUTPUT/persistence-schema" i
  # The DDL carries the schema change; the surrounding column definitions make
  # the diff unambiguously a stored-format migration rather than a probe.
  {
    printf 'CREATE TABLE account (\n    id INTEGER PRIMARY KEY,\n    email TEXT NOT NULL\n);\n\n'
    printf 'CREATE TABLE session (\n    id INTEGER PRIMARY KEY,\n    account_id INTEGER NOT NULL REFERENCES account(id)\n);\n\n'
    for i in $(seq 1 40); do
      printf 'CREATE TABLE audit_%02d (\n    id INTEGER PRIMARY KEY,\n    account_id INTEGER NOT NULL,\n    payload TEXT NOT NULL\n);\n\n' \
        "$i"
    done
  } > "$dir/base/schema.sql"
  {
    printf 'CREATE TABLE account (\n    id INTEGER PRIMARY KEY,\n    email TEXT NOT NULL,\n    slug TEXT NOT NULL DEFAULT (hex(id)),\n    created_at INTEGER NOT NULL DEFAULT 0\n);\n\n'
    printf 'CREATE TABLE session (\n    id INTEGER PRIMARY KEY,\n    account_id INTEGER NOT NULL REFERENCES account(id),\n    expires_at INTEGER NOT NULL DEFAULT 0\n);\n\n'
    for i in $(seq 1 40); do
      printf 'CREATE TABLE audit_%02d (\n    id INTEGER PRIMARY KEY,\n    account_id INTEGER NOT NULL,\n    payload TEXT NOT NULL,\n    recorded_at INTEGER NOT NULL DEFAULT 0\n);\n\n' \
        "$i"
    done
  } > "$dir/head/schema.sql"
  {
    printf '//! Storage access for the account table.\n\n'
    printf 'pub const SELECT_ACCOUNT: &str = "SELECT id, email FROM account";\n\n'
    for i in $(seq 1 30); do
      printf 'pub const SELECT_AUDIT_%02d: &str = "SELECT id, account_id, payload FROM audit_%02d";\n' \
        "$i" "$i"
    done
    printf '\npub fn load(conn: &Connection, id: i64) -> Option<Account> {\n    conn.query(SELECT_ACCOUNT, &[id]).ok()\n}\n'
  } > "$dir/base/src/store.rs"
  {
    printf '//! Storage access for the account table.\n\n'
    printf 'pub const SELECT_ACCOUNT: &str = "SELECT id, email, slug, created_at FROM account";\n\n'
    for i in $(seq 1 30); do
      printf 'pub const SELECT_AUDIT_%02d: &str = "SELECT id, account_id, payload, recorded_at FROM audit_%02d";\n' \
        "$i" "$i"
    done
    printf '\npub fn load(conn: &Connection, id: i64) -> Option<Account> {\n    conn.query(SELECT_ACCOUNT, &[id]).ok()\n}\n\npub fn load_by_slug(conn: &Connection, slug: &str) -> Option<Account> {\n    conn.query("SELECT id, email, slug, created_at FROM account WHERE slug = ?", &[slug]).ok()\n}\n'
  } > "$dir/head/src/store.rs"
  printf 'ALTER TABLE account ADD COLUMN slug TEXT;\n' > "$dir/base/migrations/0001_init.sql"
  printf 'ALTER TABLE account ADD COLUMN slug TEXT;\n' > "$dir/head/migrations/0001_init.sql"
  printf 'ALTER TABLE account ADD COLUMN slug TEXT NOT NULL DEFAULT (hex(id));\nALTER TABLE account ADD COLUMN created_at INTEGER NOT NULL DEFAULT 0;\nALTER TABLE session ADD COLUMN expires_at INTEGER NOT NULL DEFAULT 0;\n' \
    > "$dir/head/migrations/0002_account_slug.sql"
  for i in $(seq 1 20); do
    printf 'ALTER TABLE audit_%02d ADD COLUMN recorded_at INTEGER NOT NULL DEFAULT 0;\n' "$i"
  done > "$dir/head/migrations/0003_audit_recorded_at.sql"
}

emit_concurrency() {
  begin_case concurrency src
  local dir="$OUTPUT/concurrency" i
  {
    printf 'use std::sync::{Arc, Mutex};\n\n'
    printf 'pub struct Slot {\n    id: usize,\n    checked_out: bool,\n}\n\n'
    printf 'pub struct Pool {\n    slots: Arc<Mutex<Vec<Slot>>>,\n}\n\n'
    printf 'impl Pool {\n'
    for i in $(seq 1 25); do
      printf '    pub fn acquire_%02d(&self) -> Option<usize> {\n        let mut slots = self.slots.lock().expect("pool mutex poisoned");\n        let slot = slots.iter_mut().find(|slot| !slot.checked_out && slot.id %% 26 == %d)?;\n        slot.checked_out = true;\n        Some(slot.id)\n    }\n\n' \
        "$i" "$i"
    done
    printf '}\n'
  } > "$dir/base/src/pool.rs"
  {
    printf 'use std::sync::{Arc, RwLock};\n\n'
    printf 'pub struct Slot {\n    id: usize,\n    checked_out: bool,\n}\n\n'
    printf 'pub struct Pool {\n    slots: Arc<RwLock<Vec<Slot>>>,\n}\n\n'
    printf 'impl Pool {\n'
    printf '    pub fn capacity(&self) -> usize {\n        let slots = self.slots.read().expect("pool lock poisoned");\n        slots.len()\n    }\n\n'
    for i in $(seq 1 25); do
      printf '    pub fn acquire_%02d(&self) -> Option<usize> {\n        let mut slots = self.slots.write().expect("pool lock poisoned");\n        let slot = slots.iter_mut().find(|slot| !slot.checked_out && slot.id %% 26 == %d)?;\n        slot.checked_out = true;\n        Some(slot.id)\n    }\n\n' \
        "$i" "$i"
    done
    printf '}\n'
  } > "$dir/head/src/pool.rs"
}

emit_error_handling() {
  begin_case error-handling src
  local dir="$OUTPUT/error-handling" module base_count head_count
  # Base: three variants per module. Head: ten more across the four modules,
  # each with its matching Display arm.
  while read -r module base_count head_count; do
    write_error_module "$dir/base/src/$module.rs" "$module" "$base_count"
    write_error_module "$dir/head/src/$module.rs" "$module" "$head_count"
  done <<'MODULES'
parse 3 6
io 3 5
net 3 5
codec 3 6
MODULES
}

emit_mixed_buried() {
  begin_case mixed-buried src
  local dir="$OUTPUT/mixed-buried" i name
  for i in $(seq 1 15); do
    name="$(printf 'm%03d' "$i")"
    if [[ "$i" -eq 8 ]]; then
      # The single risky unit: a stored-schema column change buried under the
      # same mechanical rename pattern as the other fourteen files.
      printf 'pub const DDL: &str = "CREATE TABLE t (id INTEGER PRIMARY KEY)";\n\npub fn old_of(value: OldName) -> OldName {\n    OldName::from(value)\n}\n\npub fn old_len(value: OldName) -> usize {\n    OldName::from(value).len()\n}\n' \
        > "$dir/base/src/$name.rs"
      printf 'pub const DDL: &str = "CREATE TABLE t (id INTEGER PRIMARY KEY, slug TEXT NOT NULL)";\n\npub fn new_of(value: NewName) -> NewName {\n    NewName::from(value)\n}\n\npub fn new_len(value: NewName) -> usize {\n    NewName::from(value).len()\n}\n' \
        > "$dir/head/src/$name.rs"
    else
      printf 'pub fn old_of(value: OldName) -> OldName {\n    OldName::from(value)\n}\n\npub fn old_len(value: OldName) -> usize {\n    OldName::from(value).len()\n}\n\npub fn old_is_empty(value: OldName) -> bool {\n    OldName::from(value).len() == 0\n}\n' \
        > "$dir/base/src/$name.rs"
      printf 'pub fn new_of(value: NewName) -> NewName {\n    NewName::from(value)\n}\n\npub fn new_len(value: NewName) -> usize {\n    NewName::from(value).len()\n}\n\npub fn new_is_empty(value: NewName) -> bool {\n    NewName::from(value).len() == 0\n}\n' \
        > "$dir/head/src/$name.rs"
    fi
  done
}

emit_router_invalid() {
  # Same shape as `internal-refactor`; the benchmark drives it with the failing
  # provider, so the class exists to prove a provider failure cannot downgrade
  # a mechanical rename.
  emit_rename_tree router-invalid OldName NewName 7
}

emit_docs_only
emit_tests_only
emit_lockfile_only
emit_internal_refactor
emit_behavior_change
emit_public_api_break
emit_security
emit_persistence_schema
emit_concurrency
emit_error_handling
emit_mixed_buried
emit_router_invalid

# The manifest mirrors the minimal corpus schema exactly, so both corpora parse
# through the same benchmark path.
CASES=(
  "docs-only|cheap|documentation edit with no behavior change"
  "tests-only|cheap|test-only edit with no production behavior change"
  "lockfile-only|cheap|lockfile dependency bump with no source edit"
  "internal-refactor|focused|internal rename with no public contract change"
  "behavior-change|focused|internal behavior changes without touching the public contract"
  "public-api-break|deep|public function signature changes: callers must be updated"
  "security|deep|credential handling and command construction are security sensitive"
  "persistence-schema|deep|stored schema changes: existing rows must still be readable"
  "concurrency|deep|lock ordering and shared state change"
  "error-handling|focused|failure path changes: the error branch is reachable differently"
  "mixed-buried|deep|one schema-touching unit buried among mechanical edits"
  "router-invalid|deep|provider failure must not downgrade a public contract change"
)

{
  printf '{\n  "cases": [\n'
  first=yes
  for entry in "${CASES[@]}"; do
    IFS='|' read -r name route reason <<<"$entry"
    if [[ "$first" == "yes" ]]; then
      first=no
    else
      printf ',\n'
    fi
    printf '    {"case":"%s","expected_min_route":"%s","reason":"%s"}' \
      "$name" "$route" "$reason"
    if [[ ! -d "$OUTPUT/$name" ]]; then
      printf '\ngenerate-large-fixtures: case %s was never emitted\n' "$name" >&2
      exit 1
    fi
  done
  printf '\n  ]\n}\n'
} > "$OUTPUT/manifest.json"

cp "$SOURCE_CORPUS/fake-router.sh" "$OUTPUT/fake-router.sh"
chmod +x "$OUTPUT/fake-router.sh"

printf 'generated %d cases under %s\n' "${#CASES[@]}" "$OUTPUT"
