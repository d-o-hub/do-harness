#!/usr/bin/env bash
# Measure when-changed baseline selection and runtime performance over representative change cases.
# Does NOT modify production selection logic or CLI schemas.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
BIN="${DO_HARNESS_BIN:-$ROOT/target/release/do-harness}"
MANIFEST="${1:-$ROOT/tests/fixtures/selection-impact/manifest.json}"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT/target/selection-impact-target}"
OUT_FILE="${2:-}"

if [[ ! -f "$BIN" ]]; then
  echo "Building do-harness binary..." >&2
  cargo build --manifest-path "$ROOT/Cargo.toml" --bin do-harness --release >&2
fi

OS_NAME="$(uname -s)"
ARCH_NAME="$(uname -m)"
CPU_CLASS="$(lscpu 2>/dev/null | grep -i "Model name" | head -n1 | cut -d':' -f2 | xargs || echo "$ARCH_NAME")"
RUST_VER="$(rustc --version 2>/dev/null || echo "unknown")"
CARGO_VER="$(cargo --version 2>/dev/null || echo "unknown")"

# Execute baseline measurement using python helper to safely handle JSON parsing and system timings
python3 - "$MANIFEST" "$BIN" "$ROOT" "$OS_NAME" "$ARCH_NAME" "$CPU_CLASS" "$RUST_VER" "$CARGO_VER" "$OUT_FILE" << 'EOF'
import sys
import json
import subprocess
import time
import os

manifest_path = sys.argv[1]
bin_path = sys.argv[2]
root_dir = sys.argv[3]
os_name = sys.argv[4]
arch_name = sys.argv[5]
cpu_class = sys.argv[6]
rust_ver = sys.argv[7]
cargo_ver = sys.argv[8]
out_file = sys.argv[9] if len(sys.argv) > 9 and sys.argv[9] else None

with open(manifest_path, 'r', encoding='utf-8') as f:
    manifest = json.load(f)

environment_meta = {
    "os": os_name,
    "architecture": arch_name,
    "cpu_class": cpu_class,
    "rustc_version": rust_ver,
    "cargo_version": cargo_ver,
    "cache_state": "warm"
}

results = []

cases = manifest.get("cases", [])
for case in cases:
    case_id = case["id"]
    change_class = case.get("change_class", "unknown")
    case_root = os.path.abspath(os.path.join(root_dir, case.get("root", ".")))
    changed_files = case.get("changed_files", [])

    # 1. Run do-harness explain --root <root> --set verification --format json
    explain_cmd = [bin_path, "explain", "--root", case_root, "--set", "verification", "--format", "json"]

    t0 = time.perf_counter()
    try:
        explain_proc = subprocess.run(explain_cmd, cwd=case_root, capture_output=True, text=True, check=True)
        explain_data = json.loads(explain_proc.stdout)
    except Exception as e:
        explain_data = {"selected": [], "skipped": [], "error": str(e)}

    # 2. Run do-harness verify --root <root> --set verification --format json
    verify_cmd = [bin_path, "verify", "--root", case_root, "--set", "verification", "--format", "json"]
    try:
        verify_proc = subprocess.run(verify_cmd, cwd=case_root, capture_output=True, text=True)
        t1 = time.perf_counter()
        verify_wall_ms = round((t1 - t0) * 1000.0, 2)
        verify_data = json.loads(verify_proc.stdout) if verify_proc.stdout else {}
    except Exception as e:
        t1 = time.perf_counter()
        verify_wall_ms = round((t1 - t0) * 1000.0, 2)
        verify_data = {"error": str(e)}

    selected_sensors = [s["name"] for s in explain_data.get("selected", [])]
    skipped_sensors = [s["name"] for s in explain_data.get("skipped", [])]

    sensor_durations = {}
    if "results" in verify_data:
        for r in verify_data["results"]:
            sensor_durations[r.get("sensor", "")] = r.get("duration_ms", 0)

    # Check whether sensors detected observable effect
    observable_effect = case.get("planted_defect") is not None

    record = {
        "case_id": case_id,
        "change_class": change_class,
        "changed_paths": [f["path"] for f in changed_files],
        "selected_sensors": selected_sensors,
        "skipped_sensors": skipped_sensors,
        "sensor_durations_ms": sensor_durations,
        "total_wall_ms": verify_wall_ms,
        "observable_effect_detected": observable_effect,
        "environment": environment_meta
    }
    results.append(record)

output_json = json.dumps({"schema_version": 1, "environment": environment_meta, "cases": results}, indent=2)

if out_file:
    with open(out_file, 'w', encoding='utf-8') as f:
        f.write(output_json)

print(output_json)
EOF
