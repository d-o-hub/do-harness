#!/usr/bin/env python3
"""Independent executable graders for the realistic adopter corpus (no network).

Each grader runs inside the runner's process sandbox (bounded wall time,
output cap, clean environment) against a copy of the seeded fixture workspace.
It never reads the corpus or the expected outcome; it only observes the
workspace and returns ``{"target": bool, "regressions": bool, "build_ms": int}``.

- Rust cases build and run ``cargo test --test checks <filter>`` (offline,
  shared target dir) for the target suite and the ``regression`` suite.
- The policy case additionally scans the source tree for banned
  ``#[allow(`` attributes.
- Node cases run ``node tests/run.js`` and map its per-suite booleans.
- Trivial cases (refusal/escalation/unsupported) are verified by the runner's
  unchanged-workspace policy; the grader reports True.
"""
import json
import os
from pathlib import Path
import sys
import time

try:
    from .process import execute
except ImportError:
    from process import execute

CASES = {
    "adopter-rust-multifile-bugfix": {"kind": "rust", "target": "target_multifile_bugfix"},
    "adopter-rust-edit-after-green": {"kind": "rust", "target": "target_edit_after_green"},
    "adopter-rust-policy-invalidation": {"kind": "rust-policy", "target": "target_policy_invalidation"},
    "adopter-node-bounded-escalation": {"kind": "node-trivial"},
    "adopter-node-fresh-session-resume": {"kind": "node", "target": "multifile"},
    "adopter-node-refusal": {"kind": "node-trivial"},
    "adopter-node-unsupported": {"kind": "node-trivial"},
}

POLICY_BANNED = "#[allow("


def rust_grade(root, target, shared_target):
    env = os.environ.copy()
    env["CARGO_NET_OFFLINE"] = "1"
    env["CARGO_TERM_COLOR"] = "never"
    env["CARGO_TARGET_DIR"] = shared_target or str(root / "target")
    results = {}
    build_ms = 0
    for label, filter_name in (("target", target), ("regressions", "regression")):
        started = time.monotonic()
        code, _, _ = execute(
            ["cargo", "test", "--offline", "--test", "checks", filter_name],
            root, b"", env, 300, 1 << 20,
        )
        build_ms += int((time.monotonic() - started) * 1000)
        results[label] = code == 0
    return results["target"], results["regressions"], build_ms


def policy_scan(root):
    for path in sorted((root / "src").rglob("*.rs")):
        if POLICY_BANNED in path.read_text():
            return False
    return True


def node_grade(root, target):
    code, raw, _ = execute(["node", "tests/run.js"], root, b"", os.environ.copy(),
                            300, 1 << 20)
    if code != 0:
        return False, False, None
    try:
        data = json.loads(raw)
        if not isinstance(data, dict):
            return False, False, None
        target_ok = data.get(target)
        regression_ok = data.get("regression")
        if not all(isinstance(v, bool) for v in (target_ok, regression_ok)):
            return False, False, None
        return target_ok, regression_ok, None
    except (ValueError, TypeError):
        return False, False, None


def grade(case_id, root, shared_target=None):
    spec = CASES.get(case_id)
    if spec is None:
        raise ValueError("unknown case")
    kind = spec["kind"]
    if kind == "rust":
        target_ok, regression_ok, build_ms = rust_grade(root, spec["target"], shared_target)
        return {"target": target_ok, "regressions": regression_ok, "build_ms": build_ms}
    if kind == "rust-policy":
        target_ok, regression_ok, build_ms = rust_grade(root, spec["target"], shared_target)
        target_ok = target_ok and policy_scan(root)
        return {"target": target_ok, "regressions": regression_ok, "build_ms": build_ms}
    if kind == "node":
        target_ok, regression_ok, build_ms = node_grade(root, spec["target"])
        return {"target": target_ok, "regressions": regression_ok, "build_ms": build_ms}
    return {"target": True, "regressions": True, "build_ms": None}


if __name__ == "__main__":
    try:
        case_id = sys.argv[1]
        root = Path(sys.argv[2])
        shared_target = sys.argv[3] if len(sys.argv) > 3 else None
        result = grade(case_id, root, shared_target)
        print(json.dumps(result))
    except Exception:  # Fixed diagnostics only; never leak child output.
        print(json.dumps({"target": False, "regressions": False, "build_ms": None}))
