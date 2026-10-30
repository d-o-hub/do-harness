#!/usr/bin/env python3
"""Unit tests for the CI workflow cost report generator and baseline analyzer."""

import io
import json
import pathlib
import sys
import tempfile
import unittest
from unittest import mock

SCRIPTS = pathlib.Path(__file__).resolve().parents[1]
sys.path.insert(0, str(SCRIPTS))

from ci_workflow_costs import (  # noqa: E402
    DISCLAIMER,
    calculate_cost_report,
    format_text_report,
    load_baseline,
    main as ci_workflow_costs_main,
)

SAMPLE_BASELINE_JSON = {
    "schema_version": 1,
    "sample_metadata": {
        "sampled_runs": 10,
        "successful_runs": 8,
        "failed_runs": 2,
        "workflows": ["verify.yml", "scheduled.yml"],
    },
    "ephemeral_state_disclaimer": DISCLAIMER,
    "baseline_metrics": {
        "verify.yml": {
            "jobs": {
                "verify": {
                    "runner": "ubuntu-latest",
                    "median_duration_seconds": 200,
                    "steps": {
                        "checkout": {"duration_seconds": 5},
                        "build": {"duration_seconds": 195},
                    },
                },
                "dsh_bundle": {
                    "runner": "ubuntu-latest",
                    "median_duration_seconds": 150,
                    "steps": {
                        "build": {"duration_seconds": 150},
                    },
                },
            }
        },
        "scheduled.yml": {
            "jobs": {
                "scheduled": {
                    "runner": "ubuntu-24.04",
                    "median_duration_seconds": 500,
                    "steps": {
                        "verify": {"duration_seconds": 500},
                    },
                }
            }
        },
    },
    "top_cost_contributors": [
        {
            "rank": 1,
            "category": "Scheduled Coverage",
            "workflow": "scheduled.yml",
            "impact": "High duration",
        }
    ],
    "proven_optimizations": [
        {
            "id": "consolidate_dsh_bundle",
            "description": "Consolidate dsh_bundle into verify job.",
            "estimated_savings_seconds": 150,
            "metric_kind": "historical_estimate",
            "coverage_preserved": "Runs node tests on release binary.",
        }
    ],
    "retained_checks_matrix": {
        "MSRV": "Check and test on Rust 1.85",
    },
}


class TestCiWorkflowCosts(unittest.TestCase):
    def test_calculate_cost_report(self):
        report = calculate_cost_report(SAMPLE_BASELINE_JSON)

        self.assertEqual(report["disclaimer"], DISCLAIMER)
        # Total baseline = 200 + 150 + 500 = 850 runner-seconds
        self.assertEqual(report["total_baseline_duration_seconds"], 850)
        self.assertEqual(report["total_runner_duration_seconds"], 850)
        # Estimated savings = 150 (projection)
        self.assertEqual(report["total_estimated_savings_seconds"], 150)
        # Measured savings without attributable runs = 0
        self.assertEqual(report["total_measured_savings_seconds"], 0)
        # Projected = 850 - 150 = 700
        self.assertEqual(report["projected_duration_seconds"], 700)
        # Savings percentage = (150 / 850) * 100 = 17.6%
        self.assertEqual(report["savings_percentage"], 17.6)

        self.assertIn("verify.yml", report["workflows"])
        self.assertIn("scheduled.yml", report["workflows"])
        self.assertEqual(
            report["workflows"]["verify.yml"]["total_median_duration_seconds"], 350
        )

    def test_format_text_report(self):
        report = calculate_cost_report(SAMPLE_BASELINE_JSON)
        text = format_text_report(report)

        self.assertIn("CI WORKFLOW COST & SAVINGS REPORT", text)
        self.assertIn("Baseline Total Runner Compute: 850 runner-seconds", text)
        self.assertNotIn("Proven Savings From Optimization", text)
        self.assertIn("Estimated / Projected Savings: 150s (17.6%) (projection)", text)
        self.assertIn("Measured Savings: 0s", text)
        self.assertIn("Projected Median Duration Total: 700s", text)
        self.assertIn("#1 [scheduled.yml] Scheduled Coverage", text)
        self.assertIn("consolidate_dsh_bundle: -150s [HISTORICAL ESTIMATE / PROJECTION]", text)
        self.assertIn("MSRV: Check and test on Rust 1.85", text)

    def test_measured_savings_with_attributable_runs(self):
        measured_sample = {
            "schema_version": 2,
            "sample_metadata": {
                "collection_window": {"start": "2026-10-14T00:00:00Z", "end": "2026-10-15T00:00:00Z"},
                "attributable_sample": True,
            },
            "baseline_metrics": {
                "verify.yml": {"jobs": {"verify": {"median_duration_seconds": 100, "steps": {}}}},
            },
            "top_cost_contributors": [],
            "proven_optimizations": [
                {
                    "id": "real_step_cache",
                    "description": "Cache step",
                    "measured_savings_seconds": 25,
                    "metric_kind": "measured_runner_seconds",
                    "attributable_runs": {
                        "before_run_id": "111000",
                        "before_timestamp": "2026-10-14T10:00:00Z",
                        "after_run_id": "111001",
                        "after_timestamp": "2026-10-14T11:00:00Z",
                    },
                    "coverage_preserved": "Identical step output.",
                }
            ],
            "retained_checks_matrix": {},
        }
        report = calculate_cost_report(measured_sample)
        self.assertEqual(report["total_measured_savings_seconds"], 25)
        text = format_text_report(report)
        self.assertIn("Measured Savings: 25s", text)
        self.assertIn("before run 111000", text)
        self.assertIn("after run 111001", text)
    def test_load_baseline_valid_and_invalid(self):
        with tempfile.NamedTemporaryFile("w+", suffix=".json") as tmp:
            tmp.write(json.dumps(SAMPLE_BASELINE_JSON))
            tmp.flush()

            data = load_baseline(pathlib.Path(tmp.name))
            self.assertEqual(data["schema_version"], 1)

        with tempfile.NamedTemporaryFile("w+", suffix=".json") as tmp:
            tmp.write(json.dumps({"incomplete": True}))
            tmp.flush()

            with self.assertRaises(ValueError):
                load_baseline(pathlib.Path(tmp.name))

    def test_cli_check_mode(self):
        with tempfile.NamedTemporaryFile("w+", suffix=".json") as tmp:
            tmp.write(json.dumps(SAMPLE_BASELINE_JSON))
            tmp.flush()

            argv = ["ci_workflow_costs.py", "--baseline", tmp.name, "--check"]
            output = io.StringIO()
            with mock.patch.object(sys, "argv", argv):
                with mock.patch.object(sys, "stdout", output):
                    with self.assertRaises(SystemExit) as raised:
                        ci_workflow_costs_main()
            self.assertEqual(raised.exception.code, 0)
            self.assertIn("OK: ci_workflow_costs check passed", output.getvalue())


if __name__ == "__main__":
    unittest.main()
