#!/usr/bin/env bash
# Optional typed semantic router for review-depth selection.
# Wraps external router executable PR_TRIAGE_ROUTER with strict input selection,
# validation, deterministic route policy, and head-bound caching.
set -euo pipefail

if [[ $# -lt 1 ]]; then
  echo "usage: semantic-route.sh <pr-number|--base ... --head ...> [options]" >&2
  exit 1
fi

DO_HARNESS_BIN="${DO_HARNESS_BIN:-do-harness}"
CONFIDENCE_THRESHOLD="${PR_TRIAGE_CONFIDENCE_THRESHOLD:-0.80}"
ROUTER_TIMEOUT="${PR_TRIAGE_ROUTER_TIMEOUT:-10}"

exec python3 - "$DO_HARNESS_BIN" "$CONFIDENCE_THRESHOLD" "$ROUTER_TIMEOUT" "$@" <<'PY'
import json
import math
import os
import subprocess
import sys

def fallback(reason, source="residual", route="deep"):
    return {
        "status": "fallback",
        "route": route,
        "source": source,
        "router_confidence": 0.0,
        "judgment": None,
        "reason": reason,
    }

def get_git_dir():
    try:
        res = subprocess.run(
            ["git", "rev-parse", "--git-dir"],
            capture_output=True,
            text=True,
            check=True,
        )
        return res.stdout.strip()
    except Exception:
        return ".git"

def get_raw_diff(pr_num, merge_base, head_sha):
    if merge_base and head_sha:
        try:
            res = subprocess.run(
                ["git", "diff", "--no-color", "--find-renames", "--unified=3", merge_base, head_sha],
                capture_output=True,
                text=True,
            )
            if res.returncode == 0 and res.stdout:
                return res.stdout
        except Exception:
            pass
    if pr_num is not None:
        try:
            res = subprocess.run(
                ["gh", "pr", "diff", str(pr_num)],
                capture_output=True,
                text=True,
            )
            if res.returncode == 0:
                return res.stdout
        except Exception:
            pass
    return ""

def validate_confidence(val):
    if not isinstance(val, (int, float)):
        return False
    if math.isnan(val) or math.isinf(val):
        return False
    if val < 0.0 or val > 1.0:
        return False
    return True

def validate_judgment(judgment):
    if not isinstance(judgment, dict):
        return False
    if judgment.get("schema_version") != 1:
        return False

    valid_change_kinds = {
        "docs", "tests", "dependency", "internal",
        "public-api", "security", "mixed", "unknown"
    }
    if judgment.get("change_kind") not in valid_change_kinds:
        return False

    if not validate_confidence(judgment.get("confidence")):
        return False

    answer_fields = [
        "behavior_change",
        "public_contract_change",
        "security_sensitive",
        "needs_repository_context",
    ]
    valid_answers = {"yes", "no", "unknown"}

    for field in answer_fields:
        obj = judgment.get(field)
        if not isinstance(obj, dict):
            return False
        if obj.get("answer") not in valid_answers:
            return False
        if not validate_confidence(obj.get("confidence")):
            return False

    return True

def derive_route(judgment, threshold):
    top_conf = judgment["confidence"]
    if top_conf < threshold:
        return "deep"

    answer_fields = [
        "behavior_change",
        "public_contract_change",
        "security_sensitive",
        "needs_repository_context",
    ]
    for field in answer_fields:
        if judgment[field]["confidence"] < threshold:
            return "deep"

    sec = judgment["security_sensitive"]["answer"]
    pub = judgment["public_contract_change"]["answer"]
    ctx = judgment["needs_repository_context"]["answer"]
    beh = judgment["behavior_change"]["answer"]
    kind = judgment["change_kind"]

    if sec == "yes" or pub == "yes" or ctx == "yes":
        return "deep"

    if kind in ("docs", "tests") and beh != "yes":
        return "cheap"

    if beh == "yes":
        return "focused"

    return "focused"

def main():
    do_harness_bin = sys.argv[1]
    try:
        threshold = float(sys.argv[2])
    except ValueError:
        threshold = 0.80
    try:
        timeout_sec = float(sys.argv[3])
    except ValueError:
        timeout_sec = 10.0

    target_args = sys.argv[4:]

    git_dir = get_git_dir()
    cache_dir = os.path.join(git_dir, "pr-triage", "routes")
    os.makedirs(cache_dir, exist_ok=True)

    cmd = [do_harness_bin, "pr", "review"] + target_args + ["--format", "json"]
    try:
        harness_res = subprocess.run(cmd, capture_output=True, text=True)
        if harness_res.returncode != 0:
            print(json.dumps(fallback("harness_pr_review_failed")))
            return
        report = json.loads(harness_res.stdout)
    except Exception as e:
        print(json.dumps(fallback(f"harness_exec_error: {e}")))
        return

    pr_num = report.get("pr")
    head_sha = report.get("head", "")
    merge_base = report.get("merge_base", "")
    review_schema_version = report.get("schema_version", 3)
    policy_sha256 = report.get("policy", {}).get("sha256", "none")
    router_schema_version = 1

    measurement = report.get("measurement", {})
    verdict = measurement.get("verdict", "no-go")

    if verdict == "reduced":
        source = "residual"
        content = report.get("residual", [])
    else:
        source = "raw"
        content = get_raw_diff(pr_num, merge_base, head_sha)

    cache_key = {
        "pr": pr_num if pr_num is not None else " ".join(target_args),
        "head_sha": head_sha,
        "merge_base": merge_base,
        "review_schema_version": review_schema_version,
        "policy_sha256": policy_sha256,
        "router_schema_version": router_schema_version,
    }

    cache_file_name = f"{pr_num if pr_num is not None else 'range'}.json"
    cache_file = os.path.join(cache_dir, cache_file_name)
    if os.path.exists(cache_file):
        try:
            with open(cache_file, "r") as f:
                cached_data = json.load(f)
            if cached_data.get("identity") == cache_key:
                print(json.dumps(cached_data["result"]))
                return
        except Exception:
            pass

    router_bin = os.environ.get("PR_TRIAGE_ROUTER", "").strip()
    if not router_bin:
        res = fallback("router_not_configured", source=source, route="deep")
        print(json.dumps(res))
        return

    if not os.path.isfile(router_bin) or not os.access(router_bin, os.X_OK):
        res = fallback("executable_not_found_or_not_executable", source=source, route="deep")
        print(json.dumps(res))
        return

    input_payload = {
        "schema_version": router_schema_version,
        "pr": pr_num,
        "head_sha": head_sha,
        "merge_base": merge_base,
        "source": source,
        "content": content,
    }

    try:
        router_proc = subprocess.run(
            [router_bin],
            input=json.dumps(input_payload),
            capture_output=True,
            text=True,
            timeout=timeout_sec,
        )
        if router_proc.returncode != 0:
            res = fallback("router_process_failed", source=source, route="deep")
            print(json.dumps(res))
            return
        router_stdout = router_proc.stdout
    except subprocess.TimeoutExpired:
        res = fallback("router_timeout", source=source, route="deep")
        print(json.dumps(res))
        return
    except Exception as e:
        res = fallback(f"router_exec_error: {e}", source=source, route="deep")
        print(json.dumps(res))
        return

    try:
        judgment = json.loads(router_stdout)
    except Exception:
        res = fallback("malformed_json_output", source=source, route="deep")
        print(json.dumps(res))
        return

    if not validate_judgment(judgment):
        res = fallback("invalid_judgment_schema", source=source, route="deep")
        print(json.dumps(res))
        return

    route = derive_route(judgment, threshold)
    result = {
        "status": "ok",
        "route": route,
        "source": source,
        "router_confidence": judgment["confidence"],
        "judgment": judgment,
    }

    try:
        with open(cache_file, "w") as f:
            json.dump({"identity": cache_key, "result": result}, f)
    except Exception:
        pass

    print(json.dumps(result))

if __name__ == "__main__":
    main()
PY
