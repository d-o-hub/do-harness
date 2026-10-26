"""Versioned paired workflow measurements, separate from stochastic claims."""
import argparse
import hashlib
import json
import math
import os
from pathlib import Path
import platform
import subprocess
import sys
import tempfile
import time

from .process import execute

ROOT = Path(__file__).resolve().parents[2]
CORPUS = ROOT / "benchmarks/workflow-v1.json"
GRADER = Path(__file__).with_name("grade.py")
EXIT_ACTION = {"changed": 0, "refused": 10, "escalated": 11}
MAX_FILES = 128
MAX_FILE_BYTES = 1048576


def digest(value):
    return hashlib.sha256(json.dumps(value, sort_keys=True).encode()).hexdigest()


def load_corpus(path):
    data = json.loads(path.read_text())
    budget = data["budgets"]
    if any(type(budget[key]) is not int for key in ("max_attempts", "wall_seconds", "output_bytes")):
        raise ValueError("invalid budget types")
    if (data["schema_version"] != 1 or data["rubric_version"] != 1
            or not 1 <= budget["max_attempts"] <= 3
            or not 1 <= budget["wall_seconds"] <= 120
            or not 512 <= budget["output_bytes"] <= MAX_FILE_BYTES
            or not 1 <= len(data["cases"]) <= 64):
        raise ValueError("invalid corpus")
    ids = set()
    for case in data["cases"]:
        if case["id"] in ids or case["expected"] not in EXIT_ACTION:
            raise ValueError("invalid case")
        ids.add(case["id"])
        if len(case["description"]) > 2000:
            raise ValueError("oversized task")
        if not case["control"] or not case["treatment"]:
            raise ValueError("empty workflow fixture")
        for files in [case["files"], *case["control"], *case["treatment"]]:
            for name, text in files.items():
                path = Path(name)
                if (not name or "\\" in name or ":" in name or path.is_absolute()
                        or ".." in path.parts or len(text) > MAX_FILE_BYTES):
                    raise ValueError("invalid fixture path")
    return data


def environment(allowed=()):
    # Credentials are opt-in for the agent only, never inherited by graders.
    keys = ("PATH", "SYSTEMROOT", "WINDIR", "COMSPEC", "PYTHONHOME", "LANG")
    env = {key: os.environ[key] for key in (*keys, *allowed) if key in os.environ}
    env["PYTHONDONTWRITEBYTECODE"] = "1"
    return env


def write_files(root, files):
    for name, text in files.items():
        path = root / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text)


def workspace_digest(root):
    files = {}
    pending = [root]
    visited = 0
    try:
        while pending:
            with os.scandir(pending.pop()) as entries:
                for entry in entries:
                    visited += 1
                    if visited > MAX_FILES or entry.is_symlink():
                        return None
                    if entry.is_dir(follow_symlinks=False):
                        pending.append(Path(entry.path))
                    elif entry.is_file(follow_symlinks=False) and entry.stat().st_size <= MAX_FILE_BYTES:
                        path = Path(entry.path)
                        files[path.relative_to(root).as_posix()] = hashlib.sha256(path.read_bytes()).hexdigest()
                    else:
                        return None
    except OSError:
        return None
    return digest(files)


def grade(case, root, seconds, limit):
    code, raw, reason = execute([sys.executable, str(GRADER), case["id"], str(root)],
                                root, b"", environment(), seconds, limit)
    try:
        result = json.loads(raw)
        if (code != 0 or reason or set(result) != {"target", "regressions"}
                or any(type(v) is not bool for v in result.values())):
            raise ValueError("invalid grade")
        return result
    except (ValueError, TypeError):
        return {"target": False, "regressions": False}


def run_case(case, workflow, command, budgets, allowed):
    started = time.monotonic()
    deadline = started + budgets["wall_seconds"]
    with tempfile.TemporaryDirectory(prefix="workflow-benchmark-") as directory:
        root = Path(directory)
        write_files(root, case["files"])
        initial = workspace_digest(root)
        verification_started = time.monotonic()
        prior = grade(case, root, deadline - time.monotonic(), budgets["output_bytes"])
        verification_seconds = time.monotonic() - verification_started
        if not prior["regressions"]:
            raise ValueError("fixture has failing pre-existing checks")
        first = final = regression = False
        checks = 1
        reason = "attempt-budget"
        feedback = []
        for attempt in range(1, budgets["max_attempts"] + 1):
            if command is None:
                patches = case[workflow]
                if attempt > len(patches):
                    break
                write_files(root, patches[attempt - 1])
                code, failure = EXIT_ACTION[case["expected"]], None
            else:
                request = {"protocol_version": 1, "case": digest(case["id"])[:12],
                           "task": case["description"], "workspace": str(root),
                           "attempt": attempt, "feedback": feedback}
                code, _, failure = execute(command, root, json.dumps(request).encode(),
                    environment(allowed), deadline - time.monotonic(), budgets["output_bytes"])
            verification_started = time.monotonic()
            result = grade(case, root, deadline - time.monotonic(), budgets["output_bytes"])
            verification_seconds += time.monotonic() - verification_started
            checks += 1
            regression |= not result["regressions"]
            unchanged = initial is not None and workspace_digest(root) == initial
            policy = case["expected"] == "changed" or unchanged
            final = (failure is None and code == EXIT_ACTION[case["expected"]]
                     and result["target"] and result["regressions"] and policy)
            if attempt == 1:
                first = final
            if final:
                reason = "completed"
                break
            if failure:
                reason = failure
                break
            feedback = [key + "-failed" for key, ok in result.items() if not ok]
        return {"case_id": case["id"], "kind": case["kind"], "first_attempt_pass": first,
                "final_success": final, "regression_seen": regression, "attempts": attempt,
                "repair_cycles": max(0, attempt - 1), "verification_cost": checks,
                "elapsed_ms": int((time.monotonic() - started) * 1000) if command else None,
                "verification_ms": int(verification_seconds * 1000) if command else None,
                "timing": "measured-wall-clock" if command else "unmeasured-fixture",
                "escalated_unresolved": not final, "reason": reason,
                "token_cost": None, "currency_cost": None,
                "usage_reason": "provider usage unavailable in adapter protocol"}


def interval(successes, total):
    """Wilson 95% binomial interval: descriptive, not a paired causal estimate."""
    z = 1.96
    ratio = successes / total
    scale = 1 + z * z / total
    center = (ratio + z * z / (2 * total)) / scale
    radius = z * math.sqrt(ratio * (1 - ratio) / total + z * z / (4 * total * total)) / scale
    return [round(center - radius, 6), round(center + radius, 6)]


def summarize(rows):
    count = len(rows)
    success = sum(row["final_success"] for row in rows)
    first_passes = sum(r["first_attempt_pass"] for r in rows)
    return {"scenario_count": count, "successes": success, "failures": count - success,
            "first_attempt_pass_rate": first_passes / count,
            "first_attempt_95_interval": interval(first_passes, count),
            "final_success_rate": success / count, "final_success_95_interval": interval(success, count),
            "regression_rate": sum(r["regression_seen"] for r in rows) / count,
            "attempts": sum(r["attempts"] for r in rows),
            "verification_cost": sum(r["verification_cost"] for r in rows)}


def benchmark(data, baseline=None, treatment=None, allowed=()):
    if (baseline is None) != (treatment is None):
        raise ValueError("both workflow commands are required")
    rows = {"control": [], "treatment": []}
    commands = {"control": baseline, "treatment": treatment}
    # Alternate paired order to avoid always assigning the first run to control.
    for index, case in enumerate(data["cases"]):
        order = ("control", "treatment") if index % 2 == 0 else ("treatment", "control")
        for workflow in order:
            rows[workflow].append(run_case(case, workflow, commands[workflow], data["budgets"], allowed))
    env = environment()
    for key in list(env):
        if key.startswith("GIT_"):
            env.pop(key)
    revision = subprocess.run(["git", "rev-parse", "HEAD"], cwd=ROOT, env=env,
                              capture_output=True, text=True, check=False)
    mode = "external-agent" if baseline else "deterministic-fixture"
    return {"schema_version": 1, "benchmark_version": data["benchmark_version"],
            "rubric_version": data["rubric_version"], "corpus_sha256": digest(data),
            "grader_sha256": hashlib.sha256(GRADER.read_bytes()).hexdigest(),
            "commit": revision.stdout.strip() if revision.returncode == 0 else None,
            "environment": {"python": platform.python_version(), "os": platform.system(),
                            "machine": platform.machine(), "external_model_version": None},
            "mode": mode, "budgets": data["budgets"],
            "verification_cost_unit": "grader-invocations",
            "workflow_identities": {name: digest(command) if command else "fixture-" + name + "-v1"
                                    for name, command in commands.items()},
            "comparability": "same corpus, grader, environment, mode and budgets required",
            "interpretation": "descriptive paired sample; no causal or general-performance claim",
            "workflows": {name: {"summary": summarize(values), "cases": values}
                          for name, values in rows.items()}}


def command_json(text):
    value = json.loads(text)
    if not isinstance(value, list) or not value or any(not isinstance(v, str) for v in value):
        raise argparse.ArgumentTypeError("expected a nonempty JSON argv list")
    return value


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("--corpus", type=Path, default=CORPUS)
    parser.add_argument("--baseline-command-json", type=command_json)
    parser.add_argument("--treatment-command-json", type=command_json)
    parser.add_argument("--allow-env", action="append", default=[])
    args = parser.parse_args()
    if args.self_test:
        import unittest
        from . import tests
        result = unittest.TextTestRunner().run(unittest.defaultTestLoader.loadTestsFromModule(tests))
        return int(not result.wasSuccessful())
    try:
        report = benchmark(load_corpus(args.corpus), args.baseline_command_json,
                           args.treatment_command_json, args.allow_env)
        print(json.dumps(report, sort_keys=True))
        if report["mode"] == "deterministic-fixture":
            return int(any(w["summary"]["failures"] for w in report["workflows"].values()))
        return 0  # Stochastic measures are a trend report, never a one-run threshold.
    except (ValueError, KeyError, OSError, TypeError):
        print("benchmark: invalid corpus or execution contract", file=sys.stderr)
        return 2
