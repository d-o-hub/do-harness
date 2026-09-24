#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"
report_dir=target/semver-checks-reports
mkdir -p "$report_dir"

echo "cargo-semver-checks version:"
cargo semver-checks --version

check_crate() {
  local crate="$1"
  local log="$report_dir/$crate.log"
  local output status baseline_version display_version

  echo "=== Checking API compatibility for $crate ==="
  if output=$(cargo semver-checks check-release -p "$crate" 2>&1); then
    printf '%s\n' "$output" | tee "$log"
    return 0
  else
    status=$?
  fi

  printf '%s\n' "$output" | tee "$log"
  if [[ "$crate" != "do-harness" || "$output" != *"no library targets found in package"* ]]; then
    return "$status"
  fi

  baseline_version=$(printf '%s\n' "$output" | python3 -c 'import re, sys; match = re.search(r"Building do-harness v(\S+) \(baseline\)", sys.stdin.read()); print(match.group(1) if match else "")')
  if [[ "$baseline_version" != "0.1.2" ]]; then
    display_version="$baseline_version"
    if [[ -z "$display_version" ]]; then display_version=unknown; fi
    echo "FAIL: do-harness baseline v$display_version has no library target; only the known v0.1.2 empty API baseline is modeled." >&2
    return "$status"
  fi

  echo "Registry do-harness v0.1.2 has no library target; comparing its empty public API against the committed baseline representation." | tee -a "$log"
  (
    baseline_root=$(mktemp -d)
    trap 'rm -rf "$baseline_root"' EXIT
    mkdir -p "$baseline_root/src"
    cp "$root/scripts/fixtures/semver/do-harness-0.1.2/Cargo.toml.template" "$baseline_root/Cargo.toml"
    cp "$root/scripts/fixtures/semver/do-harness-0.1.2/src/lib.rs" "$baseline_root/src/lib.rs"
    cargo semver-checks check-release -p do-harness \
      --baseline-root "$baseline_root" 2>&1 | tee -a "$log"
  )
}

check_crate do-harness-types
check_crate do-harness-db
check_crate do-harness
