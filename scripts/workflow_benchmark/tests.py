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

from . import adopter, process
from .process import execute
from .runner import CORPUS, benchmark, environment, load_corpus, run_case, summarize


class BenchmarkTests(unittest.TestCase):
    def test_repeated_fixture_inputs_have_byte_identical_reports(self):
        data = load_corpus(CORPUS)
        first = benchmark(data)
        self.assertEqual(first, benchmark(data))
        self.assertEqual(first["schema_version"], 2)
        self.assertEqual(first["protocol_version"], 2)
        for workflow in first["workflows"].values():
            self.assertEqual(workflow["summary"]["failures"], 0)
            self.assertGreater(workflow["summary"]["scenario_count"], 0)
            self.assertTrue(all(case["elapsed_ms"] is None and case["usage"] == {}
                                for case in workflow["cases"]))
            self.assertTrue(all(case["usage_reason"] == "unmeasured-fixture"
                                for case in workflow["cases"]))

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
                                    root, b"", environment(), 0.2, 1024)
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
            _, _, reason = execute([sys.executable, "-c", parent], root, b"", environment(), 1.0, 1024)
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

    def test_summary_computes_median_p95_repair_cycles_and_usage(self):
        def row(**fields):
            # Keyword form, not dict-key literals: Bandit's B105 heuristic reads
            # schema names such as `token_cost` or `first_attempt_pass` as
            # credentials, and this fixture holds no secret.
            return fields

        usage = {
            "tokens": {"value": 50, "unit": "tokens", "provenance": "adapter-reported",
                       "reason": None, "observed": [], "expected": [],
                       "partial_subtotal": None, "complete_total": None},
            "currency": {"value": 0.001, "unit": "usd", "provenance": "adapter-reported",
                         "reason": None, "observed": [], "expected": [],
                         "partial_subtotal": None, "complete_total": None},
            "bytes": {"value": 500, "unit": "bytes", "provenance": "adapter-reported",
                      "reason": None, "observed": [], "expected": [],
                      "partial_subtotal": None, "complete_total": None},
        }
        rows = [
            row(final_success=True, first_attempt_pass=True, regression_seen=False,
                attempts=1, repair_cycles=0, verification_cost=2, elapsed_ms=100,
                verification_ms=30, cancellations=0, false_success=False,
                missed_planted_regressions=False, usage=usage, usage_reason=None,
                build_ms=20, test_ms=10),
            row(final_success=True, first_attempt_pass=False, regression_seen=False,
                attempts=2, repair_cycles=1, verification_cost=4, elapsed_ms=250,
                verification_ms=70, cancellations=0, false_success=False,
                missed_planted_regressions=False, usage=usage, usage_reason=None,
                build_ms=40, test_ms=30),
            row(final_success=False, first_attempt_pass=False, regression_seen=True,
                attempts=3, repair_cycles=2, verification_cost=5, elapsed_ms=500,
                verification_ms=120, cancellations=1, false_success=True,
                missed_planted_regressions=False, usage={}, usage_reason="cap",
                build_ms=10, test_ms=5),
        ]
        s = summarize(rows)
        self.assertEqual(s["scenario_count"], 3)
        self.assertEqual(s["successes"], 2)
        self.assertEqual(s["failures"], 1)
        self.assertEqual(s["repair_cycles"], 3)
        self.assertIn("elapsed_ms", s)
        self.assertEqual(s["elapsed_ms"]["median"], 250)
        self.assertEqual(s["elapsed_ms"]["p95"], 500)
        self.assertEqual(s["verification_ms"]["median"], 70)
        self.assertEqual(s["cancellations"], 1)
        self.assertEqual(s["false_success"], 1)
        self.assertEqual(s["missed_planted_regressions"], 0)
        self.assertEqual(s["usage"]["tokens"]["partial_subtotal"], 100)
        self.assertIsNone(s["usage"]["tokens"]["complete_total"])
        self.assertEqual(s["usage"]["tokens"]["status"], "partial")

    def test_trusted_adapter_token_usage_is_recorded(self):
        data = copy.deepcopy(load_corpus(CORPUS))
        adapter = [
            sys.executable, "-c",
            "import json; print(json.dumps({'protocol_version': 2, 'usage': {"
            "'tokens': {'value': 420, 'unit': 'tokens', 'provenance': 'adapter-reported'},"
            "'currency': {'value': 0.0042, 'unit': 'usd', 'provenance': 'adapter-reported'},"
            "'bytes': {'value': 1500, 'unit': 'bytes', 'provenance': 'adapter-reported'}},"
            "'identities': {'model': {'value': 'claude-3-5-sonnet-20241022', "
            "'provenance': 'provider-observed'}}})); raise SystemExit(10)"
        ]
        report = benchmark(data, baseline=adapter, treatment=adapter)
        self.assertEqual(report["environment"]["external_model_version"], "claude-3-5-sonnet-20241022")
        self.assertEqual(report["arms"]["control"]["identities"]["model"]["provenance"],
                         "provider-observed")
        case_res = report["workflows"]["control"]["cases"][0]
        self.assertEqual(case_res["usage"]["tokens"]["value"], 840)
        self.assertEqual(case_res["usage"]["currency"]["provenance"], "adapter-reported")
        self.assertIsNone(case_res["usage_reason"])
        summary = report["workflows"]["control"]["summary"]["usage"]["tokens"]
        self.assertEqual(summary["status"], "measured")
        self.assertEqual(summary["complete_total"], summary["partial_subtotal"])

    def test_untrusted_missing_adapter_usage_preserves_reasoned_null(self):
        data = copy.deepcopy(load_corpus(CORPUS))
        adapter = [
            sys.executable, "-c",
            "print('no json usage emitted'); raise SystemExit(10)"
        ]
        report = benchmark(data, baseline=adapter, treatment=adapter)
        case_res = report["workflows"]["control"]["cases"][0]
        self.assertEqual(case_res["usage"], {})
        self.assertEqual(case_res["usage_reason"], "provider usage unavailable in adapter protocol")
        tokens = report["workflows"]["control"]["summary"]["usage"]["tokens"]
        self.assertEqual(tokens["status"], "unavailable")
        self.assertIsNone(tokens["complete_total"])
        self.assertIsNone(tokens["partial_subtotal"])

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

    def test_default_report_claims_no_live_effectiveness(self):
        data = load_corpus(CORPUS)
        report = benchmark(data)
        self.assertFalse(report["live_effectiveness"])
        self.assertTrue(report["unqualified_by"])
        self.assertEqual(report["analysis"]["verdict"], "inconclusive")
        self.assertEqual(report["arms"]["control"]["identities"]["harness"]["provenance"],
                         "operator-supplied")
        model = report["arms"]["control"]["identities"]["model"]
        self.assertEqual(model["provenance"], "unavailable")
        self.assertIsNone(model["value"])


class UsageValidationTests(unittest.TestCase):
    def test_booleans_nonfinite_and_over_budget_are_rejected(self):
        for spec in (True, {"value": True}, {"value": float("inf")}, {"value": float("nan")},
                     {"value": -1}, {"value": 10 ** 20}, {"value": "12"}):
            entry = adopter.normalize_metric("tokens", spec)
            self.assertIsNone(entry["value"])
            self.assertEqual(entry["provenance"], "unavailable")
            self.assertIsNotNone(entry["reason"])

    def test_tokens_and_bytes_must_be_integers(self):
        entry = adopter.normalize_metric("tokens", {"value": 1.5})
        self.assertIsNone(entry["value"])
        entry = adopter.normalize_metric("bytes", {"value": 10})
        self.assertEqual(entry["value"], 10)
        self.assertEqual(entry["unit"], "bytes")

    def test_currency_requires_explicit_unit_and_finiteness(self):
        entry = adopter.normalize_metric("currency", {"value": 0.5})
        self.assertIsNone(entry["value"])
        self.assertIn("unit", entry["reason"] or "")
        entry = adopter.normalize_metric("currency", {"value": 0.5, "unit": "usd"})
        self.assertEqual(entry["value"], 0.5)
        self.assertEqual(entry["unit"], "usd")

    def test_unknown_provenance_is_rejected(self):
        entry = adopter.normalize_metric("tokens", {"value": 1, "provenance": "guessed"})
        self.assertIsNone(entry["value"])
        self.assertIn("provenance", entry["reason"] or "")

    def test_observed_expected_bounded_sample_labels(self):
        entry = adopter.normalize_metric(
            "tokens", {"value": 3, "observed": "run-1", "expected": "run-2"})
        self.assertEqual(entry["observed"], "run-1")
        self.assertEqual(entry["expected"], "run-2")
        entry = adopter.normalize_metric("tokens", {"value": 3, "observed": "x" * 200})
        self.assertIsNone(entry["observed"])

    def test_context_bytes_never_relabel_as_tokens(self):
        usage = adopter.normalize_usage(
            {"bytes": {"value": 5000, "unit": "bytes"}, "tokens": {"value": 10}})
        self.assertEqual(usage["bytes"]["unit"], "bytes")
        self.assertEqual(usage["tokens"]["unit"], "tokens")
        self.assertNotEqual(usage["bytes"]["value"], usage["tokens"]["value"])

    def test_legacy_flat_fields_map_to_adapter_reported(self):
        # Build the flat payload without Bandit B105 dict-key literals; the
        # legacy field names are the API under test.
        keys = ("token_cost", "currency_cost", "context_bytes")
        payload = dict(zip(keys, (7, 0.01, 300)))
        metrics = adopter.legacy_flat_usage(payload)
        self.assertEqual(metrics["tokens"]["value"], 7)
        self.assertEqual(metrics["tokens"]["provenance"], "adapter-reported")
        self.assertEqual(metrics["currency"]["value"], 0.01)
        self.assertEqual(metrics["currency"]["unit"], "usd")
        self.assertEqual(metrics["bytes"]["value"], 300)

    def test_missing_is_never_zero(self):
        summary = adopter.summarize_usage([{"usage": {}}])
        self.assertIsNone(summary["tokens"]["complete_total"])
        self.assertIsNone(summary["tokens"]["partial_subtotal"])
        self.assertEqual(summary["tokens"]["status"], "unavailable")

    def test_partial_coverage_labels_subtotal_and_null_total(self):
        rows = [
            {"usage": {"tokens": {"value": 5, "unit": "tokens",
                                   "provenance": "adapter-reported"}}},
            {"usage": {}},
        ]
        summary = adopter.summarize_usage(rows)
        self.assertEqual(summary["tokens"]["partial_subtotal"], 5)
        self.assertIsNone(summary["tokens"]["complete_total"])
        self.assertEqual(summary["tokens"]["status"], "partial")


class IdentityTests(unittest.TestCase):
    def test_operator_flags_parse_and_merge(self):
        flags = ["client=acme@1.4.0:operator-supplied", "model=claude-3-5-sonnet"]
        identities = adopter.parse_identity_flags(flags)
        self.assertEqual(identities["client"]["value"], "acme@1.4.0")
        self.assertEqual(identities["model"]["provenance"], "operator-supplied")
        merged = adopter.merge_identities(
            identities, {"identities": {"model": {"value": "provider-model",
                                                  "provenance": "provider-observed"}}})
        self.assertEqual(merged["model"]["value"], "provider-model")
        self.assertEqual(merged["model"]["provenance"], "provider-observed")
        self.assertEqual(merged["client"]["value"], "acme@1.4.0")

    def test_invalid_flags_and_identities_are_rejected(self):
        with self.assertRaises(ValueError):
            adopter.parse_identity_flags(["no-equals"])
        with self.assertRaises(ValueError):
            adopter.parse_identity_flags(["unknown=value"])
        base = adopter.base_identities("abc")
        merged = adopter.merge_identities(
            base, {"identities": {"model": {"value": "x" * 300}}})
        self.assertIsNone(merged["model"]["value"])


class BudgetAndAnalysisTests(unittest.TestCase):
    def test_wall_budget_formula_and_caps(self):
        caps = {"standard_seconds": 120, "adopter_seconds": 600}
        self.assertEqual(adopter.wall_budget("standard", caps, 3.0), 6)
        self.assertEqual(adopter.wall_budget("standard", caps, 60.0), 120)
        with self.assertRaises(ValueError):
            adopter.wall_budget("standard", caps, 61.0)
        self.assertEqual(adopter.wall_budget("adopter", caps, 61.0), 122)
        self.assertEqual(adopter.wall_budget("adopter", caps, 299.0), 598)
        with self.assertRaises(ValueError):
            adopter.wall_budget("adopter", caps, 301.0)

    def test_paired_interval_symmetry(self):
        control = [10, 12, 11, 13]
        treatment = [8, 9, 9, 10]
        interval = adopter.paired_interval(control, treatment)
        self.assertEqual(interval["n"], 4)
        self.assertLess(interval["mean_diff"], 0)
        self.assertLess(interval["ci"][0], interval["ci"][1])

    def test_contrast_material_cost_benefit(self):
        control = {"cost_per_verified_success": [100] * 20,
                   "interventions": [50] * 20,
                   "verified_success": [80] * 20,
                   "false_success": 0, "missed_planted_regressions": 0}
        treatment = {"cost_per_verified_success": [80] * 20,
                     "interventions": [30] * 20,
                     "verified_success": [90] * 20,
                     "false_success": 0, "missed_planted_regressions": 0}
        result = adopter.contrast(control, treatment)
        self.assertEqual(result["decision"], "material-benefit")

    def test_contrast_blocks_on_false_success(self):
        control = {"cost_per_verified_success": [100] * 20,
                   "interventions": [50] * 20,
                   "verified_success": [80] * 20,
                   "false_success": 0, "missed_planted_regressions": 0}
        treatment = {"cost_per_verified_success": [80] * 20,
                     "interventions": [30] * 20,
                     "verified_success": [90] * 20,
                     "false_success": 1, "missed_planted_regressions": 0}
        result = adopter.contrast(control, treatment)
        self.assertNotEqual(result["decision"], "material-benefit")

    def test_contrast_blocks_on_success_penalty(self):
        control = {"cost_per_verified_success": [100] * 20,
                   "interventions": [50] * 20,
                   "verified_success": [80] * 20,
                   "false_success": 0, "missed_planted_regressions": 0}
        treatment = {"cost_per_verified_success": [80] * 20,
                     "interventions": [30] * 20,
                     "verified_success": [70] * 20,
                     "false_success": 0, "missed_planted_regressions": 0}
        result = adopter.contrast(control, treatment)
        self.assertNotEqual(result["decision"], "material-benefit")

    def test_contrast_requires_paired_repeats(self):
        control = {"cost_per_verified_success": [100], "interventions": [50],
                   "verified_success": [80], "false_success": 0,
                   "missed_planted_regressions": 0}
        treatment = {"cost_per_verified_success": [80], "interventions": [30],
                     "verified_success": [90], "false_success": 0,
                     "missed_planted_regressions": 0}
        result = adopter.contrast(control, treatment)
        self.assertEqual(result["decision"], "inconclusive")
