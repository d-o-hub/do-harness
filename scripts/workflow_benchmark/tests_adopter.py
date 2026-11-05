"""Adopter-corpus fixture tests: validation, grader directions, budgets, replay
metadata. The full corpus replay is exercised by the documented CLI command;
these tests cover the fast boundaries that gate it."""
import json
import os
from pathlib import Path
import shutil
import tempfile
import unittest

from .runner import ADOPTER_CORPUS, ROOT, load_corpus, safe_rel_path
from . import adopter


def _graded(case_id, overlay):
    """Copy the rust fixture, apply one overlay dict, run the trusted grader."""
    from .grade import grade
    with tempfile.TemporaryDirectory(prefix="adopter-grader-") as directory:
        root = Path(directory)
        fixture = ROOT / "benchmarks/fixtures/workflow-adopter-v1/rust"
        shutil.copytree(fixture, root, dirs_exist_ok=True)
        for target, source in overlay.items():
            path = root / target
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes((root / source).read_bytes())
        return grade(case_id, root, ADOPTER_CORPUS)


class AdopterCorpusTests(unittest.TestCase):
    def test_corpus_validates_and_is_versioned(self):
        data = load_corpus(ADOPTER_CORPUS)
        self.assertEqual(data["benchmark_version"], "workflow-adopter-v1")
        self.assertEqual(data["budgets"]["max_attempts"], 3)
        self.assertEqual(data["budgets"]["output_bytes"], 65536)
        self.assertIsNone(data["budgets"]["wall_seconds"])
        self.assertEqual(len(data["projects"]), 2)
        self.assertEqual(len(data["cases"]), 8)
        scenarios = {case["scenario"] for case in data["cases"]}
        self.assertEqual(scenarios, adopter.SCENARIOS)

    def test_invalid_corpus_rejections(self):
        data = json.loads(ADOPTER_CORPUS.read_text())
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "bad.json"
            clone = json.loads(json.dumps(data))
            clone["projects"][0]["fixture"] = "../escape"
            path.write_text(json.dumps(clone))
            with self.assertRaises(ValueError):
                load_corpus(path)
            clone = json.loads(json.dumps(data))
            clone["cases"][0]["project_id"] = "missing"
            path.write_text(json.dumps(clone))
            with self.assertRaises(ValueError):
                load_corpus(path)
            clone = json.loads(json.dumps(data))
            clone["budgets"]["max_attempts"] = 2
            path.write_text(json.dumps(clone))
            with self.assertRaises(ValueError):
                load_corpus(path)

    def test_safe_paths_reject_escape_absolute_and_nul(self):
        for bad in ("../x", "/abs", "a\\b", "a:b", "a\x00b", ""):
            with self.assertRaises(ValueError, msg=bad):
                safe_rel_path(bad)
        self.assertEqual(safe_rel_path("a/b"), Path("a/b"))

    def test_deliberately_wrong_filter_fails_independent_grader(self):
        # Healthy fixture: both target and regressions pass.
        healthy = _graded("adopter-rust-filter-order", {})
        self.assertTrue(healthy["target"])
        self.assertTrue(healthy["regressions"])
        # Planted reversed-order/counts variants: target fails, regressions hold.
        broken = _graded("adopter-rust-filter-order", {
            "src/filter.rs": "variants/adopter-filter-reversed.rs",
            "src/summary.rs": "variants/adopter-summary-all-counts.rs",
            "src/main.rs": "variants/adopter-main-passes-all.rs",
        })
        self.assertFalse(broken["target"])
        self.assertTrue(broken["regressions"])

    def test_replay_report_metadata_contract(self):
        report = json.loads(Path(os.environ.get("ADOPTER_REPLAY", "/home/do/.tmp-wt-458/adopter-replay.json")).read_text())
        self.assertEqual(report["schema_version"], 2)
        self.assertEqual(report["protocol_version"], 2)
        self.assertFalse(report["live_effectiveness"])
        self.assertTrue(report["unqualified_by"])
        self.assertIn("pilot", report)
        self.assertEqual(report["design"]["paired_repeats"], 20)
        for arm in report["arms"].values():
            self.assertEqual(arm["identities"]["harness"]["provenance"], "operator-supplied")
        for workflow in report["workflows"].values():
            summary = workflow["summary"]
            self.assertEqual(summary["expected_unresolved"], 4)
            self.assertEqual(summary["unexpected_failures"], 0)
            self.assertEqual(summary["false_success"], 0)
            self.assertEqual(summary["missed_planted_regressions"], 0)
