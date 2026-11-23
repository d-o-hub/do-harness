"""Deterministic corpus, regression, resource and artifact contract tests."""
import copy
import json
import os
from pathlib import Path
import sys
import tempfile
import time
import unittest
from unittest import mock

from . import process
from .process import execute
from .adopter import run_resume_case, seed_repo
from .adopter_templates_node import NODE_ROUTER_FIXED, NODE_VALIDATE_FIXED
from .runner import (CORPUS, GRADER_ADOPTER, MAX_MODEL_LEN, USAGE_CAPS, benchmark,
                     digest, environment, load_corpus, parse_adapter_usage, run_case)
ROOT = Path(__file__).resolve().parents[2]
CORPUS_ADOPTER = ROOT / "benchmarks/adopter-v1.json"


class BenchmarkTests(unittest.TestCase):
    def test_repeated_fixture_inputs_have_byte_identical_reports(self):
        data = load_corpus(CORPUS)
        first = benchmark(data)
        second = benchmark(data)
        self.assertEqual(first, second)
        self.assertEqual(first["workflow_identities"], second["workflow_identities"])
        self.assertEqual(first["workflow_identity"], second["workflow_identity"])
        for workflow in first["workflows"].values():
            self.assertEqual(workflow["summary"]["failures"], 0)
            self.assertGreater(workflow["summary"]["scenario_count"], 0)
            self.assertTrue(all(case["token_cost"] is None for case in workflow["cases"]))
            self.assertTrue(all(case["elapsed_ms"] is None for case in workflow["cases"]))

    def test_deliberate_regression_is_detected(self):
        data = copy.deepcopy(load_corpus(CORPUS))
        case = next(c for c in data["cases"] if c["id"] == "regression")
        case["treatment"] = [case["control"][0]] * data["budgets"]["max_attempts"]
        result = run_case(case, "treatment", None, data["budgets"], [])
        self.assertFalse(result["final_success"])
        self.assertTrue(result["regression_seen"])
        self.assertTrue(result["escalated_unresolved"])

    def test_output_and_wall_time_are_bounded(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            code, raw, reason = execute([sys.executable, "-c", "print('x'*1000000)"],
                                       root, b"", environment(), 2, 1024)
            self.assertEqual(reason, "output-limit")
            self.assertLessEqual(len(raw), 1024)
            _, _, reason = execute([sys.executable, "-c", "import time;time.sleep(10)"],
                                    root, b"", environment(), 0.1, 1024)
            self.assertEqual(reason, "deadline")

    def test_adapter_arguments_are_not_interpreted_by_a_shell(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            argument = "$(touch injected)"
            code, raw, reason = execute([sys.executable, "-c", "import sys;print(sys.argv[1])", argument],
                                        root, b"", environment(), 2, 1024)
            self.assertEqual((code, raw.strip(), reason), (0, argument.encode(), None))
            self.assertFalse((root / "injected").exists())

    def test_windows_cleanup_uses_system_directory(self):
        child = mock.Mock(pid=123)
        child.poll.return_value = None
        with mock.patch.object(process.os, "name", "nt"), \
             mock.patch.object(process.os.path, "isabs", return_value=True), \
             mock.patch.dict(process.os.environ, {"SystemRoot": "/trusted/windows"}), \
             mock.patch.object(process.subprocess, "run") as run:
            process.terminate(child)
        self.assertEqual(run.call_args.args[0], ["taskkill.exe", "/PID", "123", "/T", "/F"])
        self.assertEqual(run.call_args.kwargs["executable"],
                         os.path.join("/trusted/windows", "System32", "taskkill.exe"))
        child.kill.assert_called_once()
        child.wait.assert_called_once()

    def test_private_environment_and_child_output_are_not_artifacts(self):
        data = load_corpus(CORPUS)
        case = next(c for c in data["cases"] if c["id"] == "refusal")
        command = [sys.executable, "-c", "print('synthetic-private-output');raise SystemExit(10)"]
        result = run_case(case, "control", command, data["budgets"], [])
        self.assertTrue(result["final_success"])
        self.assertNotIn("synthetic-private-output", json.dumps(result))
        self.assertNotIn("GH_TOKEN", environment())

    @unittest.skipUnless(os.name == "posix", "POSIX process-group contract")
    def test_timeout_stops_a_descendant_that_ignores_termination(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            worker = root / "worker.py"
            heartbeat = root / "heartbeat"
            heartbeat.write_text("0")
            worker.write_text("import signal,time\nfrom pathlib import Path\n"
                "signal.signal(signal.SIGTERM,signal.SIG_IGN)\n"
                "while True:\n Path('heartbeat').write_text(str(time.time_ns()))\n time.sleep(.01)\n")
            parent = "import subprocess,sys,time;subprocess.Popen([sys.executable,'worker.py']);time.sleep(10)"
            _, _, reason = execute([sys.executable, "-c", parent], root, b"", environment(), .5, 1024)
            self.assertEqual(reason, "deadline")
            before = heartbeat.read_bytes()
            time.sleep(.1)
            self.assertEqual(before, heartbeat.read_bytes())

    def test_unsafe_fixture_paths_and_empty_corpora_fail_closed(self):
        data = load_corpus(CORPUS)
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "corpus.json"
            data["cases"][0]["files"] = {"../escape": "invalid"}
            path.write_text(json.dumps(data))
            with self.assertRaises(ValueError):
                load_corpus(path)
            data["cases"] = []
            path.write_text(json.dumps(data))
            with self.assertRaises(ValueError):
                load_corpus(path)
    def test_summary_computes_median_p95_repair_cycles_and_context(self):
        from .runner import summarize

        def row(**fields):
            # Keyword form, not dict-key literals: Bandit's B105 heuristic reads
            # schema names such as `token_cost` or `first_attempt_pass` as
            # credentials, and this fixture holds no secret.
            return fields

        rows = [
            row(final_success=True, first_attempt_pass=True, regression_seen=False,
                attempts=1, repair_cycles=0, verification_cost=2, elapsed_ms=100,
                verification_ms=30, token_cost=50, currency_cost=0.001, context_bytes=500),
            row(final_success=True, first_attempt_pass=False, regression_seen=False,
                attempts=2, repair_cycles=1, verification_cost=4, elapsed_ms=250,
                verification_ms=70, token_cost=150, currency_cost=0.003, context_bytes=1000),
            row(final_success=False, first_attempt_pass=False, regression_seen=True,
                attempts=3, repair_cycles=2, verification_cost=5, elapsed_ms=500,
                verification_ms=120, token_cost=300, currency_cost=0.006, context_bytes=1500),
        ]
        s = summarize(rows)
        self.assertEqual(s["scenario_count"], 3)
        self.assertEqual(s["successes"], 2)
        self.assertEqual(s["failures"], 1)
        self.assertEqual(s["repair_cycles"], 3)
        self.assertIn("elapsed_ms", s)
        self.assertEqual(s["elapsed_ms"]["median"], 250)
        self.assertEqual(s["elapsed_ms"]["p95"], 500)
        self.assertIn("verification_ms", s)
        self.assertEqual(s["verification_ms"]["median"], 70)
        self.assertEqual(s["verification_ms"]["p95"], 120)
        self.assertIn("context_bytes", s)
        self.assertEqual(s["context_bytes"]["total"], 3000)
        self.assertEqual(s["token_cost"]["total"], 500)
        self.assertEqual(s["token_cost"]["status"], "measured")

    def test_trusted_adapter_token_usage_is_recorded(self):
        data = copy.deepcopy(load_corpus(CORPUS))
        adapter = [
            sys.executable, "-c",
            "import json; print(json.dumps({'token_cost': 420, 'currency_cost': 0.0042, 'context_bytes': 1500, 'model': 'claude-3-5-sonnet-20241022'})); raise SystemExit(10)"
        ]
        report = benchmark(data, baseline=adapter, treatment=adapter)
        self.assertEqual(report["environment"]["external_model_version"], "claude-3-5-sonnet-20241022")
        case_res = report["workflows"]["control"]["cases"][0]
        self.assertEqual(case_res["token_cost"], 840)
        self.assertEqual(case_res["currency_cost"], 0.0084)
        self.assertEqual(case_res["context_bytes"], 3000)
        self.assertIsNone(case_res["usage_reason"])
        self.assertEqual(case_res["usage_provenance"], {
            "token_cost": "adapter-reported", "currency_cost": "adapter-reported",
            "context_bytes": "adapter-reported", "model": "adapter-reported",
        })
        identity = report["workflow_identity"]["control"]["model"]
        self.assertEqual(identity, {"identity": "claude-3-5-sonnet-20241022",
                                    "provenance": "adapter-reported"})

    def test_untrusted_missing_adapter_usage_preserves_reasoned_null(self):
        data = copy.deepcopy(load_corpus(CORPUS))
        adapter = [
            sys.executable, "-c",
            "print('no json usage emitted'); raise SystemExit(10)"
        ]
        report = benchmark(data, baseline=adapter, treatment=adapter)
        case_res = report["workflows"]["control"]["cases"][0]
        self.assertIsNone(case_res["token_cost"])
        self.assertIsNone(case_res["currency_cost"])
        self.assertEqual(case_res["usage_reason"], "provider usage unavailable in adapter protocol")
        summary = report["workflows"]["control"]["summary"]
        self.assertIsNone(summary["token_cost"]["total"])
        self.assertEqual(summary["token_cost"]["status"], "unavailable")

    def test_environment_carries_resource_limits_and_system_identities(self):
        data = load_corpus(CORPUS)
        report = benchmark(data)
        env = report["environment"]
        self.assertIn("python", env)
        self.assertIn("os", env)
        self.assertIn("machine", env)
        self.assertIn("cpu_count", env)
        limits = report["resource_limits"]
        self.assertEqual(limits["wall_seconds"], data["budgets"]["wall_seconds"])
        self.assertEqual(limits["max_attempts"], data["budgets"]["max_attempts"])
        self.assertEqual(limits["output_bytes"], data["budgets"]["output_bytes"])

    def test_usage_payload_fail_closed_on_invalid_numerics(self):
        invalid = [
            {"token_cost": True},
            {"currency_cost": False},
            {"context_bytes": float("nan")},
            {"token_cost": float("inf")},
            {"currency_cost": float("-inf")},
            {"token_cost": -1},
            {"token_cost": USAGE_CAPS["token_cost"] + 1},
            {"currency_cost": USAGE_CAPS["currency_cost"] + 1},
            {"context_bytes": USAGE_CAPS["context_bytes"] + 1},
            {"token_cost": "many"},
            {"token_cost": [1]},
            {"model": True},
            {"model": ""},
            {"model": "x" * (MAX_MODEL_LEN + 1)},
        ]
        for payload in invalid:
            usage, reason = parse_adapter_usage(json.dumps(payload))
            self.assertIsNone(usage, payload)
            self.assertEqual(reason, "invalid adapter usage payload")
        usage, reason = parse_adapter_usage(json.dumps(
            {"token_cost": 10, "currency_cost": 0.001,
             "context_bytes": 500, "model": "stub-model"}))
        self.assertIsNone(reason)
        self.assertEqual(usage["token_cost"], 10)
        missing, reason = parse_adapter_usage("[]")
        self.assertIsNone(missing)
        self.assertEqual(reason, "provider usage unavailable in adapter protocol")

    def test_invalid_usage_claim_fails_closed_in_report(self):
        data = copy.deepcopy(load_corpus(CORPUS))
        data["cases"] = [next(c for c in data["cases"] if c["id"] == "refusal")]
        adapter = [
            sys.executable, "-c",
            "import json,sys; json.load(sys.stdin);"
            "print(json.dumps({'token_cost': True})); raise SystemExit(10)"
        ]
        report = benchmark(data, baseline=adapter, treatment=adapter)
        row = next(r for r in report["workflows"]["control"]["cases"]
                   if r["case_id"] == "refusal")
        self.assertTrue(row["final_success"])
        self.assertEqual(row["usage_reason"], "invalid adapter usage payload")
        self.assertIsNone(row["token_cost"])
        self.assertIsNone(row["usage_provenance"]["token_cost"])
        summary = report["workflows"]["control"]["summary"]
        self.assertIsNone(summary["token_cost"]["total"])
        self.assertEqual(summary["token_cost"]["status"], "unavailable")

    def test_budget_profile_max_authorized_bounds(self):
        loaded = load_corpus(CORPUS_ADOPTER, profile="experimental-pilot")
        self.assertEqual(loaded["budgets"]["wall_seconds"], 600)
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "corpus.json"
            oversized = copy.deepcopy(loaded)
            oversized["profiles"]["experimental-pilot"]["wall_seconds"] = 1000
            path.write_text(json.dumps(oversized))
            with self.assertRaisesRegex(ValueError, "profile exceeds authorized bounds"):
                load_corpus(path, profile="experimental-pilot")
            attempts = copy.deepcopy(loaded)
            attempts["profiles"]["experimental-pilot"]["max_attempts"] = 9
            path.write_text(json.dumps(attempts))
            with self.assertRaisesRegex(ValueError, "profile exceeds authorized bounds"):
                load_corpus(path, profile="experimental-pilot")
            unbound = copy.deepcopy(loaded)
            unbound["profiles"]["experimental-pilot"].pop("max_authorized")
            path.write_text(json.dumps(unbound))
            with self.assertRaisesRegex(ValueError, "invalid budget profile"):
                load_corpus(path, profile="experimental-pilot")
            with self.assertRaisesRegex(ValueError, "unknown budget profile"):
                load_corpus(path, profile="not-a-profile")

    def test_resume_with_stub_adapter_persists_sessions_and_wall_budget(self):
        data = load_corpus(CORPUS_ADOPTER)
        resume_case = next(c for c in data["cases"] if c.get("resume"))
        stub = (
            "import json,sys\n"
            "from pathlib import Path\n"
            "sys.path.insert(0, sys.argv[1])\n"
            "from adopter_templates_node import NODE_ROUTER_FIXED, NODE_VALIDATE_FIXED\n"
            "req = json.load(sys.stdin)\n"
            "root = Path(req['workspace'])\n"
            "if req['attempt'] == 1:\n"
            "    (root / 'src/router.js').write_text(NODE_ROUTER_FIXED)\n"
            "    sys.stdout.buffer.write(b'x' * 70000)\n"
            "else:\n"
            "    (root / 'src/validate.js').write_text(NODE_VALIDATE_FIXED)\n"
            "    print(json.dumps({'token_cost': 7, 'currency_cost': 0.0007,\n"
            "                      'context_bytes': 300, 'model': 'stub-resume-model'}))\n"
        )
        command = [sys.executable, "-c", stub, str(Path(__file__).resolve().parent)]
        with tempfile.TemporaryDirectory() as directory:
            row = run_resume_case(resume_case, "control", command, data["budgets"],
                                  [], None, directory)
            self.assertTrue(row["final_success"])
            self.assertEqual(row["reason"], "completed")
            self.assertEqual(row["sessions"], 2)
            self.assertTrue(row["resumed"])
            self.assertEqual(row["attempts"], 2)
            self.assertEqual(row["token_cost"], 7)
            self.assertEqual(row["usage_provenance"]["token_cost"], "adapter-reported")
            self.assertIsNone(row["usage_reason"])
            self.assertLess(row["elapsed_ms"],
                            data["budgets"]["wall_seconds"] * 1000)
            self.assertEqual(list(Path(directory).iterdir()), [])

    def test_per_arm_workflow_identity_is_reproducible(self):
        data = copy.deepcopy(load_corpus(CORPUS))
        data["cases"] = [next(c for c in data["cases"] if c["id"] == "refusal")]
        stamped = benchmark(data, model_version="pinned-model-x")
        for arm in stamped["workflow_identity"].values():
            self.assertEqual(arm["model"],
                             {"identity": "pinned-model-x",
                              "provenance": "operator-supplied"})
        baseline = [sys.executable, "-c",
                    "import json; print(json.dumps({'token_cost': 1, 'model': 'm-a'}))"]
        treatment = [sys.executable, "-c", "print('{}')"]
        split = benchmark(data, baseline=baseline, treatment=treatment)
        control = split["workflow_identity"]["control"]
        treat = split["workflow_identity"]["treatment"]
        self.assertEqual(control["adapter"]["argv_digest"], digest(baseline))
        self.assertNotEqual(control["adapter"]["argv_digest"],
                            treat["adapter"]["argv_digest"])
        self.assertEqual(control["model"]["provenance"], "adapter-reported")
        self.assertEqual(treat["model"]["provenance"], "unavailable")

    def test_grade_adopter_detects_planted_failure_and_false_success(self):
        data = load_corpus(CORPUS_ADOPTER)
        resume_case = next(c for c in data["cases"]
                           if c["id"] == "adopter-node-fresh-session-resume")
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory) / "repo"
            seed_repo(resume_case, root)

            def grade():
                code, raw, _ = execute(
                    [sys.executable, str(GRADER_ADOPTER), resume_case["id"],
                     str(root)],
                    root, b"", os.environ.copy(), 300, 1 << 20)
                return json.loads(raw)

            # Planted failure: the pristine fixture misses the target and the
            # grader must say so while prior regression checks still hold.
            self.assertEqual(grade(),
                             {"target": False, "regressions": True, "build_ms": None})
            for patch in resume_case["control"]:
                for name, text in patch.items():
                    (root / name).write_text(text)
            self.assertEqual(grade(),
                             {"target": True, "regressions": True, "build_ms": None})
            # False-success guard: a regression-breaking change keeps the target
            # suite green; the grader must still flag the regressions.
            router = (root / "src/router.js").read_text()
            broken = router.replace("body: cached, fromCache: true", "body: cached")
            (root / "src/router.js").write_text(broken)
            self.assertEqual(grade(),
                             {"target": True, "regressions": False, "build_ms": None})
