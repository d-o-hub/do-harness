#!/usr/bin/env python3
"""Can-fail contract for effective documentation-review input accounting."""

import json
import subprocess  # nosec B404
import sys
import unittest
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]


class TestReviewInputAccounting(unittest.TestCase):
    def test_benchmark_counts_fallback_as_raw_input(self):
        result = subprocess.run(  # nosec B603 B607
            ["python3", str(REPO_ROOT / "scripts" / "docs-review-benchmark.py")],
            executable=sys.executable,
            cwd=REPO_ROOT,
            capture_output=True,
            text=True,
            check=False,
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        benchmark = json.loads(result.stdout)
        self.assertEqual(benchmark["schema_version"], 2)
        cases = benchmark["cases"]
        for case in cases:
            raw_bytes = case["full_document_input_bytes"]
            packet_consumed = (
                not case["fallback_required"] and case["packet_bytes"] < raw_bytes
            )
            expected_input = case["packet_bytes"] if packet_consumed else raw_bytes
            self.assertEqual(case["packet_consumed"], packet_consumed, case["case"])
            self.assertEqual(case["effective_review_input_bytes"], expected_input, case["case"])
            self.assertEqual(
                case["effective_reduction_bytes"], raw_bytes - expected_input, case["case"]
            )

        summary = benchmark["summary"]
        self.assertEqual(
            summary["effective_review_input_bytes"],
            sum(case["effective_review_input_bytes"] for case in cases),
        )
        self.assertEqual(
            summary["effective_reduction_bytes"],
            sum(case["effective_reduction_bytes"] for case in cases),
        )
        self.assertLess(summary["effective_reduction_bytes"], summary["candidate_reduction_bytes"])


if __name__ == "__main__":
    unittest.main()
