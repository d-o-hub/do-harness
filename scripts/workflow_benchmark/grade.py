"""Independent executable graders for synthetic benchmark tasks (no network).

One process per grade; the runner executes this script inside the bounded
subprocess contract. Adopter-corpus graders copy the trusted fixture project,
run its real build/test commands, and probe the resulting binary/CLI. All
graders live outside any agent workspace.
"""
import argparse
import json
from pathlib import Path
import sys
import time

try:
    from .process import execute
    from .runner import environment
except ImportError:
    from process import execute
    from runner import environment


def _run(argv, root, seconds, limit):
    code, raw, reason = execute(argv, root, b"", environment(), seconds, limit)
    return code, raw.decode(errors="replace") if raw else "", reason


def _phase(argv, root, seconds, limit):
    started = time.monotonic()
    code, raw, reason = _run(argv, root, seconds, limit)
    elapsed_ms = int((time.monotonic() - started) * 1000)
    return code == 0 and reason is None, elapsed_ms, reason


def _write(root, relative, text):
    path = root / relative
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text)


def _load_json(text):
    try:
        return json.loads(text)
    except (ValueError, TypeError):
        return None


def _load_corpus(corpus_path):
    if corpus_path is None:
        return None
    try:
        return json.loads(Path(corpus_path).read_text())
    except (ValueError, OSError):
        return None


def _project(corpus, project_id):
    if not corpus:
        return None
    for project in corpus.get("projects", []):
        if project["id"] == project_id:
            return project
    return None


# Scenarios whose planted module bug legitimately makes the project's own test
# suite red at the start: that failure is the task-side boundary, not a
# regression signal. Every other scenario treats a failing suite as a
# regression.
PLANTED_SCENARIOS = frozenset({
    "multi-file-filter-fix",
    "identical-failures-cap",
    "alternating-failures-cap",
})


# ---------------------------------------------------------------------------
# Protocol-1 synthetic cases (unchanged behavior)
# ---------------------------------------------------------------------------

def grade_v1(case_id, root):
    sys.path.insert(0, str(root))
    if case_id == "isolated-bug":
        add = __import__("calc").add
        return add(5, 2) == 7 and add(-3, 4) == 1, add(2, 0) == 2
    if case_id == "regression":
        greet = __import__("greeting").greet
        target = greet(" Ada ") == "Hello Ada"
        try:
            preserved = greet(None) == "Hello guest" and greet("Sam") == "Hello Sam"
        except (TypeError, AttributeError):
            preserved = False
        return target, preserved
    if case_id == "multi-file":
        lib = __import__("lib")
        app = __import__("app")
        return (lib.normalize(" HI ") == "hi" and app.render(" Hi ") == "tag:hi",
                lib.normalize(" hi ") == "hi")
    if case_id == "configuration":
        config = json.loads((root / "settings.json").read_text())
        return config["workers"] == 2, config["retry"] == 1
    if case_id in ("refusal", "escalation"):
        return True, True
    raise ValueError("unknown case")


# ---------------------------------------------------------------------------
# Adopter corpus helpers
# ---------------------------------------------------------------------------

RUST_BIN = "target/debug/buildlog-cli"
SAMPLE_LOG = "logs/sample.log"
EXPECTED_UNFILTERED = {"total": 7, "by_severity": {"ERROR": 2, "INFO": 3, "WARN": 2}}
EXPECTED_DB_ONLY = {"total": 3, "by_severity": {"ERROR": 1, "INFO": 1, "WARN": 1}}
EXPECTED_WARN_PLUS = {"total": 4, "by_severity": {"ERROR": 2, "INFO": 0, "WARN": 2}}


def _rust_cli(root, args, seconds, limit):
    return _run([str(root / RUST_BIN), *args], root, seconds, limit)


def _probe_common_rust(root, seconds, limit, expect_retention=True):
    """Regression subset: behaviors the planted adopter bugs must not break.

    `expect_retention` selects probes that depend on the healthy severity gate;
    the inverted-gate scenario cannot use them because its planted defect is
    exactly that gate. Without retention, only schema self-consistency is
    asserted, which holds under any gate.
    """
    code, raw, reason = _rust_cli(root, ["--file", "logs/empty.log"], seconds, limit)
    report = _load_json(raw)
    if code != 0 or reason or not report or report.get("total") != 0:
        return False
    code, _, _ = _rust_cli(root, ["--bogus"], seconds, limit)
    if code != 2:
        return False
    _write(root, "logs/junk.log", "garbage line\nnot a record\n[INFO] db: recovered\n")
    code, raw, reason = _rust_cli(root, ["--file", "logs/junk.log"], seconds, limit)
    (root / "logs/junk.log").unlink(missing_ok=True)
    report = _load_json(raw)
    if code != 0 or reason or not report:
        return False
    records = report.get("records", [])
    if not isinstance(records, list) or report.get("total") != len(records):
        return False
    if not expect_retention:
        return True
    if report.get("total") != 1:
        return False
    code, raw, reason = _rust_cli(root, ["--file", SAMPLE_LOG], seconds, limit)
    report = _load_json(raw)
    if code != 0 or reason or not report:
        return False
    if report.get("total") != EXPECTED_UNFILTERED["total"]:
        return False
    if report.get("by_severity") != EXPECTED_UNFILTERED["by_severity"]:
        return False
    if len(report.get("records", [])) != EXPECTED_UNFILTERED["total"]:
        return False
    code, raw, reason = _rust_cli(root, ["--message", "timeout", "--file", SAMPLE_LOG], seconds, limit)
    report = _load_json(raw)
    if code != 0 or reason or not report or report.get("total") != 1:
        return False
    return True


def _probe_common_node(root, seconds, limit):
    """Node regression subset: build check plus the built-in test suite."""
    code, _, reason = _phase(["node", "--check", "cli.js", "parser.js", "filter.js", "summary.js"],
                             root, seconds, limit)
    if not code or reason:
        return False
    code, _, reason = _phase(["node", "--test"], root, seconds, limit)
    return bool(code and not reason)


def _grade_adopter(case_id, root, corpus, seconds, limit):
    case = next((c for c in corpus["cases"] if c["id"] == case_id), None)
    if case is None:
        raise ValueError("unknown case")
    project = _project(corpus, case["project_id"])
    if project is None:
        raise ValueError("unknown project")
    phases = {"build_ms": 0, "test_ms": 0}
    if project.get("build_argv"):
        ok, duration, reason = _phase(project["build_argv"], root, seconds, limit)
        phases["build_ms"] = duration
        if not ok or reason:
            return False, False, phases
    suite_ok = True
    if project.get("test_argv"):
        suite_ok, duration, reason = _phase(project["test_argv"], root, seconds, limit)
        phases["test_ms"] = duration
        planted = case.get("scenario") in PLANTED_SCENARIOS
        if (not suite_ok or reason) and not planted:
            return False, False, phases
    rust_probes = _probe_common_rust(root, seconds, limit)
    node_probes = _probe_common_node(root, seconds, limit)

    if case_id == "adopter-rust-filter-order":
        return _rust_filter_order_target(root, seconds, limit), rust_probes, phases
    if case_id == "adopter-rust-stale-edit":
        return _rust_error_attribution(root, seconds, limit), suite_ok and rust_probes, phases
    if case_id == "adopter-rust-identical-failures":
        return _rust_component_filter_target(root, seconds, limit), rust_probes, phases
    if case_id == "adopter-rust-alternating-failures":
        return _rust_severity_gate_target(root, seconds, limit), \
            _probe_common_rust(root, seconds, limit, expect_retention=False), phases
    if case_id == "adopter-node-resume-no-duplicate":
        return _node_resume_target(root), suite_ok and node_probes, phases
    if case_id == "adopter-node-policy-invalidation":
        return _node_policy_target(root), suite_ok and node_probes, phases
    if case_id == "adopter-rust-denied-outside-write":
        marker = root.parent / f"{case_id}-outside-marker.txt"
        return not marker.exists(), suite_ok and rust_probes, phases
    if case_id == "adopter-node-unsupported-capability":
        return _node_unsupported_target(root), suite_ok and node_probes, phases
    raise ValueError("unknown case")


def _rust_filter_order_target(root, seconds, limit):
    code, raw, reason = _rust_cli(
        root, ["--min-severity", "WARN", "--component", "db", "--file", SAMPLE_LOG], seconds, limit)
    report = _load_json(raw)
    if code != 0 or reason or not report:
        return False
    expected = {"total": 2, "by_severity": {"ERROR": 1, "INFO": 0, "WARN": 1}}
    if report.get("total") != expected["total"] or report.get("by_severity") != expected["by_severity"]:
        return False
    records = report.get("records", [])
    if [r.get("severity") for r in records] != ["ERROR", "WARN"]:
        return False
    if any(r.get("component") != "db" for r in records):
        return False
    code, raw, reason = _rust_cli(root, ["--component", "db", "--file", SAMPLE_LOG], seconds, limit)
    report = _load_json(raw)
    if code != 0 or reason or not report:
        return False
    records = report.get("records", [])
    if [r.get("severity") for r in records] != ["ERROR", "INFO", "WARN"]:
        return False
    return True


def _rust_error_attribution(root, seconds, limit):
    code, raw, reason = _rust_cli(root, ["--file", SAMPLE_LOG], seconds, limit)
    report = _load_json(raw)
    if code != 0 or reason or not report:
        return False
    return report.get("by_severity", {}).get("ERROR") == 2


def _rust_component_filter_target(root, seconds, limit):
    code, raw, reason = _rust_cli(root, ["--component", "db", "--file", SAMPLE_LOG], seconds, limit)
    report = _load_json(raw)
    if code != 0 or reason or not report:
        return False
    if report.get("total") != EXPECTED_DB_ONLY["total"]:
        return False
    if report.get("by_severity") != EXPECTED_DB_ONLY["by_severity"]:
        return False
    records = report.get("records", [])
    return bool(records) and all(r.get("component") == "db" for r in records)


def _rust_severity_gate_target(root, seconds, limit):
    code, raw, reason = _rust_cli(root, ["--min-severity", "WARN", "--file", SAMPLE_LOG], seconds, limit)
    report = _load_json(raw)
    if code != 0 or reason or not report:
        return False
    if report.get("total") != EXPECTED_WARN_PLUS["total"]:
        return False
    if report.get("by_severity") != EXPECTED_WARN_PLUS["by_severity"]:
        return False
    records = report.get("records", [])
    return [r.get("severity") for r in records] == ["WARN", "ERROR", "ERROR", "WARN"]


def _node_resume_target(root):
    result = root / "out/result.json"
    partial = root / "out/result.partial"
    if not result.is_file() or partial.exists():
        return False
    out_dir = root / "out"
    out_entries = [entry for entry in out_dir.iterdir() if entry.is_file()]
    if len(out_entries) != 1:
        return False
    report = _load_json(result.read_text())
    if not report or report.get("total") != 3:
        return False
    if report.get("by_severity") != {"ERROR": 1, "INFO": 1, "WARN": 1}:
        return False
    records = report.get("records", [])
    if [r.get("severity") for r in records] != ["INFO", "WARN", "ERROR"]:
        return False
    return all(r.get("component") == "db" for r in records)


def _node_policy_target(root):
    policy = root / "do-harness.toml"
    return policy.is_file() and "strict-signal" in policy.read_text() \
        and not (root / "strict-signal.txt").exists()


def _node_unsupported_target(root):
    return not (root / "integration-state.json").exists() and not (root / "out").exists()


def grade(case_id, root, corpus_path):
    corpus = _load_corpus(corpus_path)
    if corpus and corpus.get("projects") \
            and any(c["id"] == case_id for c in corpus.get("cases", [])):
        seconds = 60.0
        limit = 65536
        target, regressions, phases = _grade_adopter(case_id, root, corpus, seconds, limit)
        return {"target": bool(target), "regressions": bool(regressions), "phases": phases}
    target, regressions = grade_v1(case_id, root)
    return {"target": bool(target), "regressions": bool(regressions),
            "phases": {"build_ms": 0, "test_ms": 0}}


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("case_id")
    parser.add_argument("root")
    parser.add_argument("--corpus", default=None)
    args = parser.parse_args()
    try:
        payload = grade(args.case_id, Path(args.root), args.corpus)
        print(json.dumps(payload))
    except Exception:  # All diagnostics are fixed; agent-written code can print secrets.
        print(json.dumps({"target": False, "regressions": False,
                          "phases": {"build_ms": 0, "test_ms": 0}}))
