"""Versioned paired workflow measurements, separate from stochastic claims."""
import argparse
import hashlib
import json
import math
import os
from pathlib import Path
import platform
import subprocess  # nosec B404 - fixed git argv, no shell
import sys
import tempfile
import time

try:
    from .process import execute
except ImportError:
    from process import execute
from .adopter import (MANIFEST, decide, manifest_repos, run_resume_case,
                      seed_repo, verify_fixture_manifest)
from .summary import summarize
ROOT = Path(__file__).resolve().parents[2]
CORPUS = ROOT / "benchmarks/workflow-v1.json"
GRADER = Path(__file__).with_name("grade.py")
GRADER_ADOPTER = Path(__file__).with_name("grade_adopter.py")
EXIT_ACTION = {"changed": 0, "refused": 10, "escalated": 11, "unsupported": 12}
MAX_FILES = 128
MAX_FILE_BYTES = 1048576
USAGE_CAPS = {"token_cost": 10**12, "currency_cost": 10**9, "context_bytes": 10**12}
MAX_MODEL_LEN = 128


def digest(value):
    return hashlib.sha256(json.dumps(value, sort_keys=True).encode()).hexdigest()

def load_corpus(path, profile=None):
    data = json.loads(path.read_text())
    budget = dict(data["budgets"])
    if any(type(budget[key]) is not int for key in ("max_attempts", "wall_seconds", "output_bytes")):
        raise ValueError("invalid budget types")
    if profile is not None:
        profiles = data.get("profiles", {})
        if profile not in profiles:
            raise ValueError("unknown budget profile")
        budget = dict(profiles[profile])
        authorized = profiles[profile].get("max_authorized", {})
        for key in ("wall_seconds", "max_attempts", "output_bytes"):
            if (type(budget[key]) is not int or not 1 <= budget[key]
                    or budget[key] > authorized.get(key, budget[key])):
                raise ValueError("profile exceeds authorized bounds")
    if (data["schema_version"] != 1 or data["rubric_version"] != 1
            or not 1 <= len(data["cases"]) <= 64):
        raise ValueError("invalid corpus")
    if profile is None and (
            not 1 <= budget["max_attempts"] <= 3
            or not 1 <= budget["wall_seconds"] <= 120
            or not 512 <= budget["output_bytes"] <= MAX_FILE_BYTES):
        raise ValueError("invalid corpus")
    if "profiles" in data:
        for name, spec in data["profiles"].items():
            if (type(spec.get("wall_seconds")) is not int
                    or type(spec.get("max_attempts")) is not int
                    or type(spec.get("output_bytes")) is not int
                    or spec["wall_seconds"] <= 0
                    or spec["max_attempts"] <= 0
                    or spec["output_bytes"] <= 0
                    or not isinstance(spec.get("recorded_from"), str)
                    or not isinstance(spec.get("max_authorized"), dict)):
                raise ValueError("invalid budget profile")
    protocol = data.get("protocol")
    if protocol is not None:
        effect = protocol.get("effect", {})
        sample = protocol.get("sample_size", {})
        if (not isinstance(protocol.get("repeats"), int) or protocol["repeats"] < 1
                or not isinstance(effect.get("tolerance"), (int, float))
                or not 0 <= effect["tolerance"] <= 1
                or not isinstance(effect.get("minimum_effect"), (int, float))
                or not isinstance(protocol.get("stopping_rule"), str)
                or not isinstance(protocol.get("decision_rule"), str)
                or not isinstance(protocol.get("exclusions"), list)
                or not isinstance(sample.get("planned_per_arm"), int)):
            raise ValueError("invalid protocol")
    if "fixtures" in data:
        if not verify_fixture_manifest():
            raise ValueError("fixture manifest mismatch")
        repos = manifest_repos()
    ids = set()
    for case in data["cases"]:
        if case["id"] in ids or case["expected"] not in EXIT_ACTION:
            raise ValueError("invalid case")
        ids.add(case["id"])
        if len(case["description"]) > 2000:
            raise ValueError("oversized task")
        if not case["control"] or not case["treatment"]:
            raise ValueError("empty workflow fixture")
        if "resume" in case and type(case["resume"]) is not bool:
            raise ValueError("invalid resume flag")
        if "repo" in case:
            if case["repo"] not in repos:
                raise ValueError("unknown fixture repo")
            files_sets = [*case["control"], *case["treatment"]]
        else:
            files_sets = [case.get("files", {}), *case["control"], *case["treatment"]]
        for files in files_sets:
            for name, text in files.items():
                path = Path(name)
                if (not name or "\\" in name or ":" in name or path.is_absolute()
                        or ".." in path.parts or len(text) > MAX_FILE_BYTES):
                    raise ValueError("invalid fixture path")
    data["budgets"] = budget
    return data


def parse_adapter_usage(raw_output):
    """Validate an adapter's JSON usage claim; reject booleans and unbounded
    numbers fail-closed. Returns ``(usage|None, reason|None)``."""
    try:
        obj = json.loads(raw_output)
    except (ValueError, TypeError):
        return None, "provider usage unavailable in adapter protocol"
    if not isinstance(obj, dict):
        return None, "provider usage unavailable in adapter protocol"
    numeric = {"token_cost": int, "currency_cost": float, "context_bytes": int}
    usage = {}
    for name, cast in numeric.items():
        if name not in obj:
            continue
        value = obj[name]
        if isinstance(value, bool) or not isinstance(value, (int, float)):
            return None, "invalid adapter usage payload"
        if not math.isfinite(float(value)) or value < 0 or value > USAGE_CAPS[name]:
            return None, "invalid adapter usage payload"
        usage[name] = cast(value) if name != "currency_cost" else float(value)
    model = obj.get("model")
    if model is not None:
        if isinstance(model, bool) or not isinstance(model, str):
            return None, "invalid adapter usage payload"
        model = model.strip()
        if not model or len(model) > MAX_MODEL_LEN:
            return None, "invalid adapter usage payload"
        usage["model"] = model
    if not usage:
        return None, "provider usage unavailable in adapter protocol"
    return usage, None


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


def grade(case, root, seconds, limit, shared_target=None):
    grader = GRADER_ADOPTER if case.get("repo") else GRADER
    argv = [sys.executable, str(grader), case["id"], str(root)]
    if shared_target is not None:
        argv.append(str(shared_target))
    code, raw, reason = execute(argv, root, b"", environment(), seconds, limit)
    try:
        result = json.loads(raw)
        if not isinstance(result, dict):
            raise ValueError("invalid grade")
        result.setdefault("build_ms", None)
        if (code != 0 or reason or set(result) != {"target", "regressions", "build_ms"}
                or any(type(v) is not bool for v in (result["target"], result["regressions"]))
                or not (result["build_ms"] is None or isinstance(result["build_ms"], (int, float)))):
            raise ValueError("invalid grade")
        return result
    except (ValueError, TypeError):
        return {"target": False, "regressions": False, "build_ms": None}


def run_case(case, workflow, command, budgets, allowed, shared_target=None):
    started = time.monotonic()
    deadline = started + budgets["wall_seconds"]
    with tempfile.TemporaryDirectory(prefix="workflow-benchmark-") as directory:
        root = Path(directory)
        if case.get("repo"):
            seed_repo(case, root)
        else:
            write_files(root, case.get("files", {}))
        initial = workspace_digest(root)
        verification_started = time.monotonic()
        prior = grade(case, root, deadline - time.monotonic(), budgets["output_bytes"],
                      shared_target)
        verification_seconds = time.monotonic() - verification_started
        if not prior["regressions"]:
            raise ValueError("fixture has failing pre-existing checks")
        first = final = regression = False
        checks = 1
        reason = "attempt-budget"
        feedback = []
        case_token_cost = None
        case_currency_cost = None
        case_context_bytes = None
        case_model = None
        usage_reason = None
        build_ms = None
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
                code, raw_output, failure = execute(command, root, json.dumps(request).encode(),
                    environment(allowed), deadline - time.monotonic(), budgets["output_bytes"])
                if raw_output:
                    usage, reason_usage = parse_adapter_usage(raw_output)
                    if usage is not None:
                        if usage.get("token_cost") is not None:
                            case_token_cost = (case_token_cost or 0) + usage["token_cost"]
                        if usage.get("currency_cost") is not None:
                            case_currency_cost = (case_currency_cost or 0.0) + usage["currency_cost"]
                        if usage.get("context_bytes") is not None:
                            case_context_bytes = (case_context_bytes or 0) + usage["context_bytes"]
                        if case_model is None and usage.get("model"):
                            case_model = usage["model"]
                        usage_reason = None
                    elif usage_reason is None:
                        usage_reason = reason_usage
            verification_started = time.monotonic()
            result = grade(case, root, deadline - time.monotonic(), budgets["output_bytes"],
                           shared_target)
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
        if usage_reason is None and command is None:
            usage_reason = "unmeasured-fixture"
        elif usage_reason is None and case_token_cost is None:
            usage_reason = "provider usage unavailable in adapter protocol"
        provenance = {
            "token_cost": "adapter-reported" if case_token_cost is not None else None,
            "currency_cost": "adapter-reported" if case_currency_cost is not None else None,
            "context_bytes": "adapter-reported" if case_context_bytes is not None else None,
            "model": "adapter-reported" if case_model is not None else None,
        }
        return {"case_id": case["id"], "kind": case["kind"], "first_attempt_pass": first,
                "final_success": final, "regression_seen": regression, "attempts": attempt,
                "repair_cycles": max(0, attempt - 1), "verification_cost": checks,
                "elapsed_ms": int((time.monotonic() - started) * 1000) if command else None,
                "verification_ms": int(verification_seconds * 1000) if command else None,
                "timing": "measured-wall-clock" if command else "unmeasured-fixture",
                "escalated_unresolved": not final, "reason": reason,
                "token_cost": case_token_cost,
                "currency_cost": round(case_currency_cost, 4) if case_currency_cost is not None else None,
                "context_bytes": case_context_bytes,
                "adapter_model": case_model,
                "usage_reason": usage_reason,
                "usage_provenance": provenance,
                "build_ms": build_ms,
                "sessions": 1, "resumed": False}


def benchmark(data, baseline=None, treatment=None, allowed=(), model_version=None,
             profile=None, resume_dir=None):
    if (baseline is None) != (treatment is None):
        raise ValueError("both workflow commands are required")
    rows = {"control": [], "treatment": []}
    commands = {"control": baseline, "treatment": treatment}
    shared_target = None
    if any(case.get("repo") for case in data["cases"]):
        shared_target = tempfile.TemporaryDirectory(prefix="adopter-target-")
    # Alternate paired order to avoid always assigning the first run to control.
    for index, case in enumerate(data["cases"]):
        order = ("control", "treatment") if index % 2 == 0 else ("treatment", "control")
        for workflow in order:
            runner_fn = run_resume_case if case.get("resume") else run_case
            row = runner_fn(case, workflow, commands[workflow], data["budgets"], allowed,
                            shared_target.name if shared_target else None, resume_dir) \
                if case.get("resume") else run_case(case, workflow, commands[workflow],
                                                    data["budgets"], allowed,
                                                    shared_target.name if shared_target else None)
            rows[workflow].append(row)
    if shared_target is not None:
        shared_target.cleanup()
    env = environment()
    for key in list(env):
        if key.startswith("GIT_"):
            env.pop(key)
    revision = subprocess.run(  # nosec B603 B607 - fixed git argv, no shell
                              ["git", "rev-parse", "HEAD"], cwd=ROOT, env=env,
                              capture_output=True, text=True, check=False)
    mode = "external-agent" if baseline else "deterministic-fixture"
    revision_sha = revision.stdout.strip() if revision.returncode == 0 else None

    effective_model = model_version
    for r_list in rows.values():
        for r in r_list:
            if r.get("adapter_model"):
                effective_model = r["adapter_model"]
                break
        if effective_model:
            break

    workflow_identity = {}
    for name, command in commands.items():
        arm_model = model_version
        model_provenance = "operator-supplied" if model_version is not None else "unavailable"
        for r in rows[name]:
            if r.get("adapter_model"):
                arm_model = r["adapter_model"]
                model_provenance = "adapter-reported"
                break
        workflow_identity[name] = {
            "client": {"name": "do-harness-workflow-benchmark-runner",
                       "version": data["benchmark_version"]},
            "adapter": {"argv_digest": digest(command) if command else None},
            "harness": {"commit": revision_sha, "python": platform.python_version(),
                        "os": platform.system(), "machine": platform.machine()},
            "model": {"identity": arm_model, "provenance": model_provenance},
        }

    grader_file = GRADER_ADOPTER if any(case.get("repo") for case in data["cases"]) else GRADER
    report = {"schema_version": 1, "benchmark_version": data["benchmark_version"],
              "rubric_version": data["rubric_version"], "corpus_sha256": digest(data),
              "grader_sha256": hashlib.sha256(grader_file.read_bytes()).hexdigest(),
              "environment": {"python": platform.python_version(), "os": platform.system(),
                              "machine": platform.machine(), "cpu_count": os.cpu_count(),
                              "external_model_version": effective_model},
              "resource_limits": {
                  "wall_seconds": data["budgets"]["wall_seconds"],
                  "max_attempts": data["budgets"]["max_attempts"],
                  "output_bytes": data["budgets"]["output_bytes"],
              },
              "mode": mode, "budgets": data["budgets"],
              "budget_profile": profile,
              "verification_cost_unit": "grader-invocations",
              "workflow_identities": {name: digest(command) if command else "fixture-" + name + "-v1"
                                      for name, command in commands.items()},
              "workflow_identity": workflow_identity,
              "comparability": "same corpus, grader, environment, mode and budgets required",
              "interpretation": "descriptive paired sample; no causal or general-performance claim",
              "workflows": {name: {"summary": summarize(values), "cases": values}
                            for name, values in rows.items()}}
    if data.get("protocol"):
        report["protocol"] = data["protocol"]
        if baseline is not None:
            report["decision"] = decide(data["protocol"], report["workflows"]["control"],
                                        report["workflows"]["treatment"])
    if data.get("fixtures"):
        try:
            report["fixture_manifest_sha256"] = json.loads(
                MANIFEST.read_text()).get("manifest_sha256")
        except (OSError, ValueError):
            report["fixture_manifest_sha256"] = None
    return report


def command_json(text):
    value = json.loads(text)
    if not isinstance(value, list) or not value or any(not isinstance(v, str) for v in value):
        raise argparse.ArgumentTypeError("expected a nonempty JSON argv list")
    return value


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("--corpus", type=Path, default=CORPUS)
    parser.add_argument("--profile", type=str, default=None)
    parser.add_argument("--resume-dir", type=Path, default=None)
    parser.add_argument("--baseline-command-json", type=command_json)
    parser.add_argument("--treatment-command-json", type=command_json)
    parser.add_argument("--allow-env", action="append", default=[])
    parser.add_argument("--model-version", type=str, default=None)
    args = parser.parse_args()
    if args.self_test:
        import unittest
        from . import tests
        result = unittest.TextTestRunner().run(unittest.defaultTestLoader.loadTestsFromModule(tests))
        return int(not result.wasSuccessful())
    try:
        report = benchmark(load_corpus(args.corpus, profile=args.profile),
                           args.baseline_command_json,
                           args.treatment_command_json, args.allow_env,
                           model_version=args.model_version, profile=args.profile,
                           resume_dir=args.resume_dir)
        print(json.dumps(report, sort_keys=True))
        if report["mode"] == "deterministic-fixture":
            return int(any(w["summary"]["failures"] for w in report["workflows"].values()))
        return 0  # Stochastic measures are a trend report, never a one-run threshold.
    except (ValueError, KeyError, OSError, TypeError):
        print("benchmark: invalid corpus or execution contract", file=sys.stderr)
        return 2
