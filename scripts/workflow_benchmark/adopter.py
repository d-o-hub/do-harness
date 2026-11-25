"""Runner-side support for the realistic adopter corpus.

Owns everything specific to repository-based cases: fixture manifest
verification, workspace seeding, fresh-session resume mechanics, and the
predeclared go/no-go/inconclusive decision rule. Imports the shared runner
helpers lazily to avoid an import cycle (runner loads first).
"""
import hashlib
import json
import math
import os
import shutil
from pathlib import Path
import tempfile
import time

ROOT = Path(__file__).resolve().parents[2]
FIXTURES = ROOT / "benchmarks/fixtures/adopter-v1"
MANIFEST = FIXTURES / "manifest.json"
MAX_SESSIONS = 3


def digest(value):
    return hashlib.sha256(json.dumps(value, sort_keys=True).encode()).hexdigest()


def digest_tree(root):
    """Whole-tree SHA-256 over every file's bytes, sorted by relative path."""
    files = {}
    for path in sorted(root.rglob("*")):
        if path.is_file():
            files[path.relative_to(root).as_posix()] = hashlib.sha256(
                path.read_bytes()
            ).hexdigest()
    return digest(files)


def verify_fixture_manifest():
    """Return True when every committed fixture repo matches its manifest digest."""
    if not MANIFEST.exists():
        return False
    manifest = json.loads(MANIFEST.read_text())
    repos = manifest.get("repos", {})
    for name, expected in repos.items():
        repo = FIXTURES / name
        if not repo.is_dir() or digest_tree(repo) != expected:
            return False
    return True


def manifest_repos():
    if not MANIFEST.exists():
        return set()
    return set(json.loads(MANIFEST.read_text()).get("repos", {}))


def seed_repo(case, root):
    """Copy the case's pinned fixture repository into the workspace."""
    shutil.copytree(FIXTURES / case["repo"], root, dirs_exist_ok=True)


# ---------------------------------------------------------------------------
# Resume mechanics
# ---------------------------------------------------------------------------

def _checkpoint_path(resume_dir, workflow, case_id):
    return Path(resume_dir) / f"{workflow}-{case_id}.json"


def _finite_nonnegative(value):
    return ((type(value) is int and value >= 0)
            or (type(value) is float and math.isfinite(value) and value >= 0))


def load_checkpoint(resume_dir, workflow, case_id):
    path = _checkpoint_path(resume_dir, workflow, case_id)
    if not path.exists():
        return None
    state = json.loads(path.read_text())
    if (not isinstance(state, dict)
            or state.get("workflow") != workflow or state.get("case_id") != case_id
            or type(state.get("attempts")) is not int or state["attempts"] < 0
            or type(state.get("sessions")) is not int or state["sessions"] < 0
            or not _finite_nonnegative(state.get("wall_remaining"))
            or not isinstance(state.get("feedback"), list)
            or any(not isinstance(item, str) for item in state["feedback"])
            or not isinstance(state.get("root"), str)
            or not isinstance(state.get("initial"), str)
            or type(state.get("checks")) is not int or state["checks"] < 1
            or not _finite_nonnegative(state.get("verification_seconds"))
            or not _finite_nonnegative(state.get("elapsed_seconds"))
            or not (state.get("build_ms") is None
                    or type(state["build_ms"]) is int and state["build_ms"] >= 0)
            or not (state.get("token_cost") is None
                    or type(state["token_cost"]) is int and state["token_cost"] >= 0)
            or not (state.get("context_bytes") is None
                    or type(state["context_bytes"]) is int and state["context_bytes"] >= 0)
            or not (state.get("currency_cost") is None
                    or _finite_nonnegative(state["currency_cost"]))
            or not (state.get("adapter_model") is None
                    or isinstance(state["adapter_model"], str))
            or not (state.get("usage_reason") is None
                    or isinstance(state["usage_reason"], str))
            or not isinstance(state.get("reason"), str)
            or type(state.get("first_attempt_pass")) is not bool
            or type(state.get("regression_seen")) is not bool
            or type(state.get("unsupported_before_interaction")) is not bool):
        raise ValueError("invalid resume checkpoint")
    return state


def save_checkpoint(resume_dir, state):
    path = _checkpoint_path(resume_dir, state["workflow"], state["case_id"])
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_name(path.name + ".tmp")
    temporary.write_text(json.dumps(state, sort_keys=True))
    os.replace(temporary, path)


def drop_checkpoint(resume_dir, workflow, case_id):
    path = _checkpoint_path(resume_dir, workflow, case_id)
    if path.exists():
        path.unlink()


def run_resume_case(case, workflow, command, budgets, allowed, shared_target=None,
                    resume_dir=None):
    """Run a fresh-session resume case across up to MAX_SESSIONS sessions.

    A session is one adapter process (or one fixture patch stage). Interrupted
    sessions persist their workspace and cumulative measurements; restoring a
    checkpoint never renews its remaining wall budget.
    """
    from .runner import (EXIT_ACTION, grade, parse_adapter_usage,
                         write_files, workspace_digest)

    started = time.monotonic()
    resume_root = Path(resume_dir).resolve() if resume_dir is not None else None
    if resume_root is not None:
        resume_root.mkdir(parents=True, exist_ok=True)
    state = (load_checkpoint(resume_root, workflow, case["id"])
             if resume_root is not None else None)
    if state is None:
        root = Path(tempfile.mkdtemp(prefix="adopter-resume-", dir=resume_root)).resolve()
        if case.get("repo"):
            seed_repo(case, root)
        else:
            write_files(root, case.get("files", {}))
        initial = workspace_digest(root)
        verification_started = time.monotonic()
        prior = grade(case, root, budgets["wall_seconds"], budgets["output_bytes"],
                      shared_target)
        verification_seconds = time.monotonic() - verification_started
        if not prior["regressions"]:
            shutil.rmtree(root, ignore_errors=True)
            raise ValueError("fixture has failing pre-existing checks")
        state = {
            "workflow": workflow, "case_id": case["id"], "root": str(root),
            "initial": initial, "attempts": 0, "feedback": [],
            "sessions": 0,
            "wall_remaining": max(0.0, budgets["wall_seconds"]
                                  - (time.monotonic() - started)),
            "verification_seconds": verification_seconds, "checks": 1,
            "elapsed_seconds": 0.0,
            "build_ms": prior["build_ms"], "token_cost": None,  # nosec B105 - usage metric
            "currency_cost": None, "context_bytes": None, "adapter_model": None,
            "usage_reason": None, "reason": "attempt-budget",
            "first_attempt_pass": False, "regression_seen": False,  # nosec B105 - pass = success
            "unsupported_before_interaction": False,
        }
    else:
        if (state["attempts"] > budgets["max_attempts"]
                or state["sessions"] > MAX_SESSIONS
                or state["wall_remaining"] > budgets["wall_seconds"]):
            raise ValueError("resume checkpoint exceeds current budget")
        root = Path(state["root"])
        if (not root.is_absolute() or root.is_symlink() or not root.is_dir()
                or not root.name.startswith("adopter-resume-")
                or root.resolve().parent != resume_root):
            raise ValueError("invalid resume checkpoint workspace")
        initial = state["initial"]

    first = state["first_attempt_pass"]
    final = False
    regression = state["regression_seen"]
    reason = state["reason"]
    feedback = list(state["feedback"])
    attempt = state["attempts"]
    sessions = state["sessions"]
    wall_left = state["wall_remaining"]
    deadline = time.monotonic() + wall_left
    checks = state["checks"]
    verification_seconds = state["verification_seconds"]
    elapsed_before = state["elapsed_seconds"]
    build_ms = state["build_ms"]
    case_token_cost = state["token_cost"]
    case_currency_cost = state["currency_cost"]
    case_context_bytes = state["context_bytes"]
    case_model = state["adapter_model"]
    usage_reason = state["usage_reason"]
    unsupported_before_interaction = state["unsupported_before_interaction"]

    def remaining():
        return max(0.0, deadline - time.monotonic())

    while (not final and sessions < MAX_SESSIONS and remaining() > 0
           and attempt < budgets["max_attempts"]):
        sessions += 1
        while not final and remaining() > 0 and attempt < budgets["max_attempts"]:
            attempt += 1
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
                code, raw_output, failure = _execute(command, root, request,
                                                     allowed, remaining(), budgets)
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
            result = grade(case, root, remaining(), budgets["output_bytes"], shared_target)
            verification_seconds += time.monotonic() - verification_started
            if result["build_ms"] is not None:
                build_ms = (build_ms or 0) + result["build_ms"]
            checks += 1
            regression |= not result["regressions"]
            unchanged = initial is not None and workspace_digest(root) == initial
            if (command is not None and code == EXIT_ACTION["unsupported"]
                    and unchanged):
                unsupported_before_interaction = True
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
            feedback = [key + "-failed" for key in ("target", "regressions")
                        if result[key] is False]
            if command is None:
                break
        wall_left = remaining()
        if not final and resume_root is not None:
            state.update({
                "attempts": attempt, "feedback": feedback, "sessions": sessions,
                "wall_remaining": wall_left, "verification_seconds": verification_seconds,
                "checks": checks, "elapsed_seconds": elapsed_before + time.monotonic() - started,
                "build_ms": build_ms, "token_cost": case_token_cost,
                "currency_cost": case_currency_cost, "context_bytes": case_context_bytes,
                "adapter_model": case_model, "usage_reason": usage_reason, "reason": reason,
                "first_attempt_pass": first, "regression_seen": regression,
                "unsupported_before_interaction": unsupported_before_interaction,
            })
            save_checkpoint(resume_root, state)

    if final:
        if resume_root is not None:
            drop_checkpoint(resume_root, workflow, case["id"])
        shutil.rmtree(root, ignore_errors=True)
    elif (resume_root is None or wall_left <= 0 or attempt >= budgets["max_attempts"]
          or sessions >= MAX_SESSIONS):
        if resume_root is not None:
            drop_checkpoint(resume_root, workflow, case["id"])
        shutil.rmtree(root, ignore_errors=True)

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
            "elapsed_ms": int((elapsed_before + time.monotonic() - started) * 1000)
            if command else None,
            "verification_ms": int(verification_seconds * 1000) if command else None,
            "timing": "measured-wall-clock" if command else "unmeasured-fixture",
            "escalated_unresolved": not final, "reason": reason,
            "token_cost": case_token_cost,
            "currency_cost": round(case_currency_cost, 4) if case_currency_cost is not None else None,
            "context_bytes": case_context_bytes, "adapter_model": case_model,
            "usage_reason": usage_reason, "usage_provenance": provenance,
            "build_ms": build_ms, "unsupported_before_interaction": unsupported_before_interaction,
            "sessions": sessions, "resumed": sessions > 1}


def _execute(command, root, request, allowed, seconds, budgets):
    from .process import execute
    from .runner import environment
    return execute(command, root, json.dumps(request).encode(), environment(allowed),
                   seconds, budgets["output_bytes"])




# ---------------------------------------------------------------------------
# Predeclared decision rule (go / no-go / inconclusive)
# ---------------------------------------------------------------------------

def decide(protocol, control, treatment):
    effect = protocol.get("effect", {})
    tolerance = effect.get("tolerance", 0.15)
    minimum_effect = effect.get("minimum_effect", tolerance)
    control_ci = control["summary"]["final_success_95_interval"]
    treatment_ci = treatment["summary"]["final_success_95_interval"]
    lo = treatment_ci[0] - control_ci[1]
    hi = treatment_ci[1] - control_ci[0]
    go_threshold = max(tolerance, minimum_effect)
    if lo > go_threshold:
        decision = "go"
    elif hi < 0:
        decision = "no-go"
    else:
        decision = "inconclusive"
    return {
        "decision": decision,
        "rationale": (
            f"treatment-control 95% delta interval [{lo:.3f}, {hi:.3f}] vs "
            f"go threshold {go_threshold} and tolerance {tolerance}; "
            f"metric {effect.get('metric', 'final_success_rate')}"
        ),
        "estimated_delta_95_interval": [round(lo, 6), round(hi, 6)],
        "tolerance": tolerance,
        "minimum_effect": minimum_effect,
        "repeats": protocol.get("repeats"),
    }
