"""Versioned paired workflow measurements, separate from stochastic claims."""
import argparse
import hashlib
import json
import math
import os
from pathlib import Path
import platform
import shutil
import subprocess  # nosec B404 - fixed git argv, no shell
import sys
import tempfile
import time

try:
    from . import adopter
    from .process import execute
except ImportError:
    import adopter
    from process import execute

ROOT = Path(__file__).resolve().parents[2]
CORPUS = ROOT / "benchmarks/workflow-v1.json"
ADOPTER_CORPUS = ROOT / "benchmarks/workflow-adopter-v1.json"
GRADER = Path(__file__).with_name("grade.py")
EXIT_ACTION = {"changed": 0, "refused": 10, "escalated": 11, "unsupported": 12}
UNRESOLVED = "unresolved"
PROTOCOL_VERSION = 2
MAX_FILES = 512
MAX_FILE_BYTES = 1048576
MAX_ARGV_ENTRIES = 16
MAX_ARGV_BYTES = 200
PRUNE_NAMES = frozenset({"target", "node_modules", "out", ".next", "dist", "build",
                         "coverage", "vendor", ".git"})
PATCH_ACTIONS = ("health:", "variant:", "policy:", "golden:", "delete:")
CANCEL_REASONS = ("deadline", "output-limit", "spawn")


def digest(value):
    return hashlib.sha256(json.dumps(value, sort_keys=True).encode()).hexdigest()


def safe_rel_path(name, allow_empty=False):
    """Reject absolute, escaping, backslash/colon, NUL or non-string names."""
    if not isinstance(name, str) or (not allow_empty and not name):
        raise ValueError("invalid fixture path")
    if "\\" in name or ":" in name or "\x00" in name:
        raise ValueError("invalid fixture path")
    path = Path(name)
    if path.is_absolute() or ".." in path.parts:
        raise ValueError("invalid fixture path")
    return path


def safe_argv(argv):
    if not isinstance(argv, list) or not argv or len(argv) > MAX_ARGV_ENTRIES:
        raise ValueError("expected a nonempty bounded argv list")
    for entry in argv:
        if not isinstance(entry, str) or not 1 <= len(entry) <= MAX_ARGV_BYTES or "\x00" in entry:
            raise ValueError("invalid argv entry")
    return argv


def load_corpus(path):
    data = json.loads(path.read_text())
    budget = data["budgets"]
    if any(type(budget[key]) is not int for key in ("max_attempts", "output_bytes")):
        raise ValueError("invalid budget types")
    if "wall_seconds" in budget and budget["wall_seconds"] is not None \
            and type(budget["wall_seconds"]) is not int:
        raise ValueError("invalid budget types")
    if (data["schema_version"] != 1 or data["rubric_version"] != 1
            or not 1 <= budget["max_attempts"] <= 3
            or not 512 <= budget["output_bytes"] <= MAX_FILE_BYTES
            or not 1 <= len(data["cases"]) <= 64):
        raise ValueError("invalid corpus")
    outcomes = {*EXIT_ACTION, UNRESOLVED}
    ids = set()
    for case in data["cases"]:
        expected = case.get("expected_outcome", case.get("expected"))
        if expected not in outcomes:
            raise ValueError("invalid case outcome")
        if case["id"] in ids:
            raise ValueError("duplicate case id")
        ids.add(case["id"])
        if len(case["description"]) > 2000:
            raise ValueError("oversized task")
        if not case.get("control") or not case.get("treatment"):
            raise ValueError("empty workflow fixture")
        for patches in (case["control"], case["treatment"]):
            for patch in patches:
                for name, value in patch.items():
                    safe_rel_path(name)
                    if isinstance(value, str) and value.startswith(PATCH_ACTIONS):
                        action, _, payload = value.partition(":")
                        if not payload:
                            raise ValueError("empty patch action payload")
                    elif not isinstance(value, str):
                        raise ValueError("patch content must be text")
        for name, text in case.get("files", {}).items():
            safe_rel_path(name)
            if len(text) > MAX_FILE_BYTES:
                raise ValueError("oversized fixture file")
    if "projects" in data:
        validate_adopter_corpus(data)
    return data
def validate_adopter_corpus(data):
    budget = data["budgets"]
    if budget["max_attempts"] != 3 or budget["output_bytes"] != 65536:
        raise ValueError("adopter budgets require attempts=3 and output=65536")
    for key in ("standard_seconds", "adopter_seconds"):
        cap = data.get("profile_caps", {}).get(key)
        if not isinstance(cap, int) or not 1 <= cap <= 86400:
            raise ValueError("invalid profile cap")
    project_ids = set()
    for project in data["projects"]:
        if project["id"] in project_ids:
            raise ValueError("duplicate project id")
        project_ids.add(project["id"])
        fixture = safe_rel_path(project["fixture"])
        resolved = (ROOT / fixture).resolve()
        if not resolved.is_relative_to(ROOT) or resolved != resolved.absolute().resolve():
            raise ValueError("invalid fixture path")
        if not resolved.is_dir():
            raise ValueError("fixture directory missing")
        safe_argv(project.get("build_argv"))
        safe_argv(project.get("test_argv"))
    for case in data["cases"]:
        if case.get("project_id") not in project_ids:
            raise ValueError("case references an unknown project")
        if case.get("scenario") not in adopter.SCENARIOS:
            raise ValueError("unknown scenario class")
        if not isinstance(case.get("grader_id"), str) or not 1 <= len(case["grader_id"]) <= 100:
            raise ValueError("invalid grader id")
        fixture = next(p["fixture"] for p in data["projects"] if p["id"] == case["project_id"])
        fixture_root = (ROOT / fixture).resolve()
        for target, source in case.get("variants", {}).items():
            safe_rel_path(target)
            safe_rel_path(source)
            if not (fixture_root / source).resolve().is_relative_to(fixture_root):
                raise ValueError("variant escapes fixture")
            if not (fixture_root / source).is_file():
                raise ValueError("variant file missing")
        for patches in (case["control"], case["treatment"]):
            for patch in patches:
                for name, value in patch.items():
                    if isinstance(value, str) and value.startswith(("variant:", "health:")):
                        _, _, payload = value.partition(":")
                        source = safe_rel_path(payload)
                        if not (fixture_root / source).resolve().is_relative_to(fixture_root) \
                                or not (fixture_root / source).is_file():
                            raise ValueError("patch source invalid")
                    elif isinstance(value, str) and value.startswith("policy:"):
                        if value.partition(":")[2] != "strict-signal":
                            raise ValueError("unknown policy patch")
                    elif isinstance(value, str) and value.startswith("golden:"):
                        if value.partition(":")[2] != "result.json":
                            raise ValueError("unknown golden patch")
                    elif isinstance(value, str) and value.startswith("delete:"):
                        safe_rel_path(value.partition(":")[2])


def environment(allowed=()):
    # Credentials are opt-in for the agent only, never inherited by graders.
    keys = ("PATH", "SYSTEMROOT", "WINDIR", "COMSPEC", "PYTHONHOME", "LANG")
    env = {key: os.environ[key] for key in (*keys, *allowed) if key in os.environ}
    env["PYTHONDONTWRITEBYTECODE"] = "1"
    return env


def write_files(root, files):
    for name, text in files.items():
        if isinstance(text, str) and text.startswith(PATCH_ACTIONS):
            continue
        path = root / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text)


def apply_patch_dict(root, fixture_root, patches):
    for name, value in patches.items():
        path = root / name
        if isinstance(value, str) and value.startswith(PATCH_ACTIONS):
            action, _, payload = value.partition(":")
            if action == "delete":
                if path.exists():
                    path.unlink()
                continue
            if action in ("health", "variant"):
                source = fixture_root / payload
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_bytes(source.read_bytes())
                continue
            if action == "policy":
                content = adopter.POLICY_STRICT_SIGNAL
            elif action == "golden":
                content = adopter.GOLDEN_RESULT_JSON
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(content)
            continue
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(value)


def copy_project(fixture_root, root):
    pending = [fixture_root]
    visited = 0
    while pending:
        source = pending.pop()
        for entry in os.scandir(source):
            visited += 1
            if visited > MAX_FILES or entry.is_symlink():
                raise ValueError("fixture too large or contains symlinks")
            if entry.is_dir(follow_symlinks=False) and entry.name in PRUNE_NAMES:
                continue
            relative = Path(entry.path).relative_to(fixture_root)
            target = root / relative
            if entry.is_dir(follow_symlinks=False):
                target.mkdir(parents=True, exist_ok=True)
                pending.append(Path(entry.path))
            elif entry.is_file(follow_symlinks=False):
                if entry.stat().st_size > MAX_FILE_BYTES:
                    raise ValueError("fixture file too large")
                target.parent.mkdir(parents=True, exist_ok=True)
                shutil.copy2(entry.path, target)
            else:
                raise ValueError("unsupported fixture entry")


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
                    if entry.is_dir(follow_symlinks=False) and entry.name in PRUNE_NAMES:
                        continue
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


def grade(case, root, seconds, limit, corpus_path=None):
    argv = [sys.executable, str(GRADER), case["id"], str(root)]
    if corpus_path is not None:
        argv += ["--corpus", str(corpus_path)]
    code, raw, reason = execute(argv, root, b"", environment(), seconds, limit)
    try:
        result = json.loads(raw)
        expected = {"target", "regressions"}
        if not expected.issubset(result) or code != 0 or reason:
            raise ValueError(f"invalid grade code={code} reason={reason} raw={raw[:200]!r}")
        if any(type(result[key]) is not bool for key in ("target", "regressions")):
            raise ValueError("invalid grade")
        phases = result.get("phases")
        if phases is None:
            phases = {"build_ms": 0, "test_ms": 0}
        elif not isinstance(phases, dict):
            raise ValueError("invalid grade")
        return {"target": result["target"], "regressions": result["regressions"],
                "phases": {"build_ms": int(phases.get("build_ms", 0) or 0),
                           "test_ms": int(phases.get("test_ms", 0) or 0)}}
    except (ValueError, TypeError):
        return {"target": False, "regressions": False,
                "phases": {"build_ms": 0, "test_ms": 0}}


def percentile(values, p):
    if not values:
        return None
    sorted_vals = sorted(values)
    rank = math.ceil(p * len(sorted_vals))
    index = max(0, min(rank - 1, len(sorted_vals) - 1))
    return sorted_vals[index]


def run_case(case, workflow, command, budgets, allowed, measure=False, corpus_path=None, projects=None):
    started = time.monotonic()
    deadline = started + budgets["wall_seconds"]
    expected = case.get("expected_outcome", case.get("expected"))
    project = projects.get(case.get("project_id")) if projects else None
    with tempfile.TemporaryDirectory(prefix="workflow-benchmark-") as directory:
        root = Path(directory)
        setup_started = time.monotonic()
        if project is not None:
            copy_project(project["fixture"], root)
        write_files(root, case.get("files", {}))
        if project is not None:
            fixture_root = project["fixture"]
            for target, source in case.get("variants", {}).items():
                path = root / target
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_bytes((fixture_root / source).read_bytes())
        if project is not None:
            apply_patch_dict(root, project["fixture"], {})
        initial = workspace_digest(root)
        setup_ms = int((time.monotonic() - setup_started) * 1000)
        verification_started = time.monotonic()
        prior = grade(case, root, max(1, deadline - time.monotonic()), budgets["output_bytes"], corpus_path)
        verification_seconds = time.monotonic() - verification_started
        if not prior["regressions"]:
            raise ValueError(f"fixture has failing pre-existing checks: {case['id']} prior={prior}")
        first = final = regression = False
        checks = 1
        cancellations = 0
        reason = "attempt-budget"
        feedback = []
        usage = {}
        identities = None
        build_ms = test_ms = 0
        for attempt in range(1, budgets["max_attempts"] + 1):
            if command is None:
                patches = case[workflow]
                if attempt > len(patches):
                    break
                apply_patch_dict(root, project["fixture"] if project else None, patches[attempt - 1])
                code, failure = EXIT_ACTION.get(expected), None
            else:
                request = {"protocol_version": PROTOCOL_VERSION, "case": digest(case["id"])[:12],
                           "task": case["description"], "workspace": str(root),
                           "attempt": attempt, "feedback": feedback,
                           "budget": {"wall_seconds": budgets["wall_seconds"],
                                       "max_attempts": budgets["max_attempts"],
                                       "output_bytes": budgets["output_bytes"]}}
                code, raw_output, failure = execute(command, root, json.dumps(request).encode(),
                    environment(allowed), deadline - time.monotonic(), budgets["output_bytes"])
                if failure in CANCEL_REASONS:
                    cancellations += 1
                if raw_output:
                    try:
                        adapter_res = json.loads(raw_output)
                        if isinstance(adapter_res, dict):
                            usage = adopter.merge_usage(usage, adopter.normalize_usage(adapter_res.get("usage")))
                            usage = adopter.merge_usage(usage, adopter.legacy_flat_usage(adapter_res))
                            identities = adopter.merge_identities(identities or adopter.base_identities(None), adapter_res)
                    except (ValueError, TypeError):
                        pass
            verification_started = time.monotonic()
            result = grade(case, root, max(1, deadline - time.monotonic()), budgets["output_bytes"], corpus_path)
            verification_seconds += time.monotonic() - verification_started
            checks += 1
            build_ms += result["phases"]["build_ms"]
            test_ms += result["phases"]["test_ms"]
            regression |= not result["regressions"]
            unchanged = initial is not None and workspace_digest(root) == initial
            policy = expected == "changed" or unchanged
            if expected == UNRESOLVED:
                final = False
            else:
                final = (failure is None and code == EXIT_ACTION[expected]
                         and result["target"] and result["regressions"] and policy)
            if attempt == 1:
                first = final
            if final:
                reason = "completed"
                break
            if failure:
                reason = failure
                break
            feedback = [key + "-failed" for key in ("target", "regressions") if not result[key]]
        usage_reason = None
        if not usage:
            usage_reason = "unmeasured-fixture" if command is None else "provider usage unavailable in adapter protocol"
        if identities is None:
            identities = adopter.base_identities(None) if command is not None else None
        false_success = bool(command is not None and code == 0 and not final)
        missed_planted = bool(expected == UNRESOLVED and final)
        return {"case_id": case["id"], "kind": case.get("scenario", case.get("kind")),
                "expected_outcome": expected, "scenario": case.get("scenario"),
                "first_attempt_pass": first, "final_success": final,
                "regression_seen": regression, "attempts": attempt,
                "repair_cycles": max(0, attempt - 1), "verification_cost": checks,
                "elapsed_ms": int((time.monotonic() - started) * 1000) if (command or measure) else None,
                "setup_ms": setup_ms, "build_ms": build_ms, "test_ms": test_ms,
                "verification_ms": int(verification_seconds * 1000) if (command or measure) else None,
                "timing": "measured-wall-clock" if (command or measure) else "unmeasured-fixture",
                "escalated_unresolved": not final, "reason": reason,
                "cancellations": cancellations, "false_success": false_success,
                "missed_planted_regressions": missed_planted,
                "usage": usage, "usage_reason": usage_reason,
                "identities": identities}


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
    repair_cycles = sum(r["repair_cycles"] for r in rows)
    attempts = sum(r["attempts"] for r in rows)
    verification_cost = sum(r["verification_cost"] for r in rows)
    cancellations = sum(r["cancellations"] for r in rows)
    false_success = sum(r["false_success"] for r in rows)
    missed = sum(r["missed_planted_regressions"] for r in rows)
    expected_unresolved = sum(
        1 for r in rows
        if r.get("expected_outcome", r.get("kind")) == UNRESOLVED and not r["final_success"])

    elapsed_vals = [r["elapsed_ms"] for r in rows if r["elapsed_ms"] is not None]
    elapsed_summary = {
        "median": percentile(elapsed_vals, 0.50),
        "p95": percentile(elapsed_vals, 0.95),
    } if elapsed_vals else None

    verification_vals = [r["verification_ms"] for r in rows if r["verification_ms"] is not None]
    verification_summary = {
        "median": percentile(verification_vals, 0.50),
        "p95": percentile(verification_vals, 0.95),
    } if verification_vals else None

    build_vals = [r["build_ms"] for r in rows if r["build_ms"] is not None]
    test_vals = [r["test_ms"] for r in rows if r["test_ms"] is not None]

    usage_status = "measured" if rows and all(r["usage_reason"] is None for r in rows) else (
        "partial" if any(r["usage"] for r in rows) else "unavailable")
    token_summary = {"total": sum(r["usage"].get("tokens", {}).get("value") or 0 for r in rows)
                     if usage_status != "unavailable" else None, "status": usage_status}

    return {"scenario_count": count, "successes": success, "failures": count - success,
            "first_attempt_pass_rate": first_passes / count,
            "first_attempt_95_interval": interval(first_passes, count),
            "final_success_rate": success / count, "final_success_95_interval": interval(success, count),
            "regression_rate": sum(r["regression_seen"] for r in rows) / count,
            "attempts": attempts,
            "repair_cycles": repair_cycles,
            "verification_cost": verification_cost,
            "cancellations": cancellations,
            "false_success": false_success,
            "missed_planted_regressions": missed,
            "expected_unresolved": expected_unresolved,
            "unexpected_failures": count - success - expected_unresolved,
            "elapsed_ms": elapsed_summary,
            "verification_ms": verification_summary,
            "build_ms": {"median": percentile(build_vals, 0.50), "p95": percentile(build_vals, 0.95)}
            if build_vals else None,
            "test_ms": {"median": percentile(test_vals, 0.50), "p95": percentile(test_vals, 0.95)}
            if test_vals else None,
            "token_cost": token_summary,
            "usage": adopter.summarize_usage(rows)}


def benchmark(data, baseline=None, treatment=None, allowed=(), identities=None,
              profile="standard", pilot=None, corpus_path=None, budgets=None):
    if (baseline is None) != (treatment is None):
        raise ValueError("both workflow commands are required")
    if budgets is None:
        budgets = data["budgets"]
    rows = {"control": [], "treatment": []}
    commands = {"control": baseline, "treatment": treatment}
    projects = None
    if "projects" in data:
        projects = {p["id"]: {"fixture": (ROOT / p["fixture"]).resolve(),
                               "build_argv": p["build_argv"], "test_argv": p["test_argv"]}
                    for p in data["projects"]}
    # Alternate paired order to avoid always assigning the first run to control.
    for index, case in enumerate(data["cases"]):
        order = ("control", "treatment") if index % 2 == 0 else ("treatment", "control")
        for workflow in order:
            rows[workflow].append(run_case(case, workflow, commands[workflow], budgets, allowed,
                                           measure="projects" in data, corpus_path=corpus_path,
                                           projects=projects))
    env = environment()
    for key in list(env):
        if key.startswith("GIT_"):
            env.pop(key)
    revision = subprocess.run(  # nosec B603 B607 - fixed git argv, no shell
                              ["git", "rev-parse", "HEAD"], cwd=ROOT, env=env,
                              capture_output=True, text=True, check=False)
    harness_revision = revision.stdout.strip() if revision.returncode == 0 else None
    mode = "external-agent" if baseline else "deterministic-fixture"

    arms = {}
    for name in ("control", "treatment"):
        base = adopter.base_identities(harness_revision)
        if identities and identities.get(name):
            for key, entry in identities[name].items():
                base[key] = entry
        for row in rows[name]:
            if row.get("identities"):
                for key, entry in row["identities"].items():
                    if entry.get("value"):
                        base[key] = entry
        arms[name] = {
            "command_sha256": digest(commands[name]) if commands[name] else "fixture-" + name + "-v1",
            "identities": base,
        }

    effective_model = None
    for name in ("control", "treatment"):
        model = arms[name]["identities"]["model"]["value"]
        if model:
            effective_model = model
            break

    return {"schema_version": 2, "protocol_version": PROTOCOL_VERSION,
            "benchmark_version": data["benchmark_version"],
            "rubric_version": data["rubric_version"], "corpus_sha256": digest(data),
            "grader_sha256": hashlib.sha256(GRADER.read_bytes()).hexdigest(),
            "commit": harness_revision,
            "environment": {"python": platform.python_version(), "os": platform.system(),
                            "machine": platform.machine(), "cpu_count": os.cpu_count(),
                            "external_model_version": effective_model},
            "resource_limits": {
                "wall_seconds": budgets["wall_seconds"],
                "max_attempts": budgets["max_attempts"],
                "output_bytes": budgets["output_bytes"],
            },
            "mode": mode, "profile": profile, "budgets": budgets,
            "pilot": pilot,
            "design": adopter.DESIGN,
            "analysis": {
                "verdict": "inconclusive",
                "reason": "single deterministic replay; no paired repeats recorded",
                "paired_repeats": 0,
            },
            "live_effectiveness": False,
            "unqualified_by": ["no qualifying live paired run; scripted/fixture replay only"],
            "verification_cost_unit": "grader-invocations",
            "comparability": "same corpus, grader, environment, mode and budgets required",
            "interpretation": "descriptive paired sample; no causal or general-performance claim",
            "arms": arms,
            "workflows": {name: {"summary": summarize(values), "cases": values}
                          for name, values in rows.items()}}


def command_json(text):
    value = json.loads(text)
    if not isinstance(value, list) or not value or any(not isinstance(v, str) for v in value):
        raise argparse.ArgumentTypeError("expected a nonempty JSON argv list")
    return value


def identity_flag(text):
    if "=" not in text:
        raise argparse.ArgumentTypeError("expected KEY=VALUE[:PROVENANCE]")
    return text


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("--corpus", type=Path, default=CORPUS)
    parser.add_argument("--baseline-command-json", type=command_json)
    parser.add_argument("--treatment-command-json", type=command_json)
    parser.add_argument("--baseline-identity", type=identity_flag, action="append", default=[])
    parser.add_argument("--treatment-identity", type=identity_flag, action="append", default=[])
    parser.add_argument("--allow-env", action="append", default=[])
    parser.add_argument("--profile", choices=("standard", "adopter"), default="standard")
    parser.add_argument("--report-json", type=Path, default=None)
    args = parser.parse_args()
    if args.self_test:
        import unittest
        from . import tests
        result = unittest.TextTestRunner().run(unittest.defaultTestLoader.loadTestsFromModule(tests))
        return int(not result.wasSuccessful())
    try:
        args.corpus = args.corpus.resolve()
        if args.report_json is not None:
            args.report_json = args.report_json.resolve()
        data = load_corpus(args.corpus)
        adopter_corpus = "projects" in data
        identities = {
            "control": adopter.parse_identity_flags(args.baseline_identity),
            "treatment": adopter.parse_identity_flags(args.treatment_identity),
        }
        budgets = data["budgets"]
        pilot = None
        if adopter_corpus:
            pilot_budgets = dict(budgets)
            pilot_budgets["wall_seconds"] = adopter.pilot_wall_seconds(args.profile, data.get("profile_caps"))
            projects = {p["id"]: {"fixture": (ROOT / p["fixture"]).resolve(),
                                   "build_argv": p["build_argv"], "test_argv": p["test_argv"]}
                        for p in data["projects"]}
            measured = {}
            for case in data["cases"]:
                result = run_case(case, "control", None, pilot_budgets, [],
                                  measure=True, corpus_path=args.corpus, projects=projects)
                measured[case["id"]] = result["elapsed_ms"]
            pilot_max_seconds = (max(measured.values()) or 0) / 1000.0
            wall = adopter.wall_budget(args.profile, data.get("profile_caps"), pilot_max_seconds)
            budgets = dict(budgets)
            budgets["wall_seconds"] = wall
            pilot = {"measured": measured, "max_end_to_end_seconds": round(pilot_max_seconds, 3),
                     "wall_budget_seconds": wall, "profile": args.profile,
                     "rule": "ceil(2 * max pilot end-to-end seconds), clamped by profile cap"}
        report = benchmark(data, args.baseline_command_json, args.treatment_command_json,
                           args.allow_env, identities=identities, profile=args.profile,
                           pilot=pilot, corpus_path=args.corpus, budgets=budgets)
        if args.report_json is not None:
            args.report_json.write_text(json.dumps(report, sort_keys=True))
        print(json.dumps(report, sort_keys=True))
        if report["mode"] == "deterministic-fixture":
            if adopter_corpus:
                bad = any(
                    w["summary"]["false_success"] or w["summary"]["missed_planted_regressions"]
                    or w["summary"]["unexpected_failures"]
                    for w in report["workflows"].values())
                return int(bad)
            return int(any(w["summary"]["failures"] for w in report["workflows"].values()))
        return 0  # Stochastic measures are a trend report, never a one-run threshold.
    except (ValueError, KeyError, OSError, TypeError):
        import traceback
        traceback.print_exc()
        print("benchmark: invalid corpus or execution contract", file=sys.stderr)
        return 2
