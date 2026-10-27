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
from .runner import CORPUS, benchmark, environment, load_corpus, run_case


class BenchmarkTests(unittest.TestCase):
    def test_repeated_fixture_inputs_have_byte_identical_reports(self):
        data = load_corpus(CORPUS)
        first = benchmark(data)
        self.assertEqual(first, benchmark(data))
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
        self.assertEqual(run.call_args.args[0],
                         [os.path.join("/trusted/windows", "System32", "taskkill.exe"),
                          "/PID", "123", "/T", "/F"])
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
            worker.write_text("import signal,time\nfrom pathlib import Path\n"
                "signal.signal(signal.SIGTERM,signal.SIG_IGN)\n"
                "while True:\n Path('heartbeat').write_text(str(time.time_ns()))\n time.sleep(.01)\n")
            parent = "import subprocess,sys,time;subprocess.Popen([sys.executable,'worker.py']);time.sleep(10)"
            _, _, reason = execute([sys.executable, "-c", parent], root, b"", environment(), .2, 1024)
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
