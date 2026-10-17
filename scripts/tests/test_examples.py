#!/usr/bin/env python3
"""
test_examples.py — Unit and integration tests for documentation example verification.
"""

import os
import sys
import unittest

# Ensure scripts directory is in sys.path
sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

from doc_tools import examples
from doc_tools.examples import (
    ExampleError,
    ExampleMarker,
    extract_example_blocks,
    is_cli_or_config_fence,
    load_fixture_manifest,
    normalize_json_output,
)


class TestExampleMarkerAndParsing(unittest.TestCase):
    def test_marker_classification_validation(self):
        # Valid executable marker
        m1 = ExampleMarker(example_id="ex1", classification="executable")
        self.assertEqual(m1.classification, "executable")
        self.assertEqual(m1.fixture_id, "ex1")

        # Valid non-executable with reason
        m2 = ExampleMarker(example_id="ex2", classification="illustrative", reason="Illustrative example")
        self.assertEqual(m2.classification, "illustrative")

        # Invalid classification
        with self.assertRaises(ExampleError):
            ExampleMarker(example_id="ex3", classification="invalid_cls")

        # Non-executable missing reason
        with self.assertRaises(ExampleError):
            ExampleMarker(example_id="ex4", classification="illustrative", reason="")

    def test_is_cli_or_config_fence(self):
        self.assertTrue(is_cli_or_config_fence("bash", "do-harness verify"))
        self.assertTrue(is_cli_or_config_fence("sh", "echo hello"))
        self.assertTrue(is_cli_or_config_fence("toml", "language = \"rust\"\n[[sensors]]"))
        self.assertTrue(is_cli_or_config_fence("json", "{\"ok\": true}"))
        self.assertFalse(is_cli_or_config_fence("python", "print('hello')"))

    def test_extract_unclassified_block_error(self):
        content = """
# Sample Doc

```bash
do-harness verify --format json
```
"""
        blocks, errors = extract_example_blocks("test.md", content)
        self.assertTrue(any("Unclassified CLI/config example block" in err for err in errors))

    def test_extract_classified_block_success(self):
        content = """
# Sample Doc

<!-- example: id=test_ex classification=illustrative reason="Sample test example" -->
```bash
do-harness verify --format json
```
"""
        blocks, errors = extract_example_blocks("test.md", content)
        self.assertEqual(len(errors), 0)
        self.assertEqual(len(blocks), 1)
        self.assertEqual(blocks[0].marker.example_id, "test_ex")


class TestFixtureManifestAndNormalization(unittest.TestCase):
    def test_load_fixture_manifest(self):
        root_dir = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
        manifest_path = os.path.join(root_dir, "tests", "fixtures", "docs-examples", "manifest.toml")

        fixtures = load_fixture_manifest(manifest_path)
        self.assertIn("generic_adoption", fixtures)
        self.assertIn("rust_init_scaffolded", fixtures)
        self.assertIn("verify_strict_evidence_freshness", fixtures)
        self.assertIn("task_workflow_db", fixtures)
        self.assertIn("usage_error", fixtures)

    def test_normalize_json_output(self):
        raw = {
            "ok": True,
            "root": "/work/dir",
            "duration_ms": 420,
            "timestamp": "2026-10-15T12:00:00Z",
            "sensors": [
                {"name": "fmt", "duration_ms": 100}
            ]
        }
        normalized = normalize_json_output(raw)
        self.assertEqual(normalized["ok"], True)
        self.assertEqual(normalized["root"], "<NORMALIZED_PATH>")
        self.assertEqual(normalized["duration_ms"], 0)
        self.assertEqual(normalized["sensors"][0]["duration_ms"], 0)


class TestFullExamplesVerification(unittest.TestCase):
    def test_verify_examples_workspace_passes(self):
        root_dir = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
        ok = examples.verify_examples(root_dir=root_dir)
        self.assertTrue(ok)


if __name__ == "__main__":
    unittest.main()
