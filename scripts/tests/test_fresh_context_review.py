#!/usr/bin/env python3
"""Hermetic self-tests for the fresh-context review adapter.

Covers packet assembly (hash stability, revision pinning), failure-path
handling (failed launch, timeout, unavailable capability, unverified
isolation, oversized output), outcome distinctness, environment scrubbing,
sanitized records, and disposition workflow. No model is ever launched; all
reviewer runs use the deterministic stub fixture.
"""
import argparse
import importlib.util
import json
import os
import sys
import tempfile
import unittest
from pathlib import Path

SCRIPT_ROOT = Path(__file__).resolve().parent.parent
REPO = SCRIPT_ROOT.parent

STUB_SCENARIOS = ("flawed", "sound", "fail", "timeout", "unavailable",
                  "leak", "huge")
LEAK_MARKER = (SCRIPT_ROOT / "fresh_context_review" / "fixtures" / "smoke"
               / "leak-marker.txt").read_text().strip()

OUTCOMES = (
    "success_with_findings",
    "success_zero_findings",
    "failure",
    "timeout",
    "not_run",
    "unverified",
)


def load(name):
    path = SCRIPT_ROOT / "fresh_context_review" / name
    spec = importlib.util.spec_from_file_location("fcr_" + name, path)
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


packet_mod = load("packet.py")
reviewer_mod = load("reviewer.py")
runner_mod = load("runner.py")
stub_mod = load("stub_reviewer.py")

CASE = {
    "id": "ut-case",
    "scenario": "flawed",
    "design_doc": "scripts/fresh_context_review/fixtures/smoke/flawed-design.md",
    "doc_paths": [
        "scripts/fresh_context_review/fixtures/smoke/flawed-design.md",
        "scripts/fresh_context_review/fixtures/smoke/flawed-expected.json",
    ],
    "objective": "Delegated admin permissions apply transitively and revoke immediately.",
    "proposed_contract": "grant_delegation / revoke_delegation / effective_permissions",
    "constraints": ["revocation must be immediate"],
    "open_questions": ["Is eventual consistency acceptable?"],
}


def run_case(**overrides):
    case = dict(CASE)
    case.update(overrides)
    args = argparse_namespace(case)
    return runner_mod.run_one_review(str(REPO), args, case=case)


def argparse_namespace(case):
    return argparse.Namespace(
        stub=case.get("scenario"),
        reviewer_command_json=None,
        parent_marker=case.get("parent_marker"),
        packet_out=None,
        meta="{}",
        design_doc=None,
        revision=None,
        constraint=[],
        proposed_contract=None,
        open_question=[],
        source_path=[],
        test_path=[],
        doc_path=[],
        limit_timeout=None,
        limit_findings=None,
        limit_output=None,
    )




class FreshContextReviewTests(unittest.TestCase):

    def test_outcomes_are_distinct(self):
        self.assertEqual(len(set(OUTCOMES)), len(OUTCOMES))
        self.assertEqual(set(OUTCOMES), set(reviewer_mod.OUTCOMES))

    def test_clean_env_scrubs_session_vars(self):
        fake = {
            "PATH": "/usr/bin",
            "HOME": "/home/x",
            "SSH_AUTH_SOCK": "/tmp/agent.sock",
            "OPENAI_API_KEY": "secret",
            "SESSION_TALK": "1",
        }
        env = reviewer_mod.clean_env(fake)
        self.assertNotIn("SSH_AUTH_SOCK", env)
        self.assertNotIn("OPENAI_API_KEY", env)
        self.assertNotIn("SESSION_TALK", env)
        self.assertIn("PATH", env)

    def test_packet_hash_stable(self):
        p1 = packet_mod.build_packet(
            repo=str(REPO), revision="HEAD", **CASE,
            source_paths=[], test_paths=[], intended_behavior="",
            timeout_s=300, max_findings=5, max_output_bytes=65536,
        )
        p2 = packet_mod.build_packet(
            repo=str(REPO), revision="HEAD", **CASE,
            source_paths=[], test_paths=[], intended_behavior="",
            timeout_s=300, max_findings=5, max_output_bytes=65536,
        )
        self.assertEqual(packet_mod.packet_hash(p1), packet_mod.packet_hash(p2))
        h1 = packet_mod.attach_hash(p1)
        h2 = packet_mod.attach_hash(p2)
        self.assertEqual(h1["packet_hash"], h2["packet_hash"])
        # Hash excludes the hash field itself: re-hashing the hashed packet
        # yields the same identity.
        self.assertEqual(packet_mod.packet_hash(h1), h1["packet_hash"])
        # Canonical serialization is byte-for-byte stable.
        self.assertEqual(packet_mod.canonical_json(h1),
                         packet_mod.canonical_json(h2))

    def test_packet_hash_sensitive_to_input(self):
        def build(objective):
            return packet_mod.attach_hash(packet_mod.build_packet(
                repo=str(REPO), revision="HEAD", **{**CASE, "objective": objective},
                source_paths=[], test_paths=[], intended_behavior="",
                timeout_s=300, max_findings=5, max_output_bytes=65536,
            ))
        self.assertNotEqual(build("objective A")["packet_hash"],
                            build("objective B")["packet_hash"])

    def test_revision_pinning_fails_closed(self):
        with self.assertRaises(ValueError):
            packet_mod.build_packet(
                repo=str(REPO), revision="definitely-not-a-revision", **CASE,
                source_paths=[], test_paths=[], intended_behavior="",
                timeout_s=300, max_findings=5, max_output_bytes=65536,
            )
        # A path absent at the pinned revision is a hard error, never a
        # silently dropped input.
        with self.assertRaises(ValueError):
            packet_mod.build_packet(
                repo=str(REPO), revision="HEAD", **CASE,
                source_paths=["scripts/does-not-exist.py"], test_paths=[],
                intended_behavior="", timeout_s=300, max_findings=5,
                max_output_bytes=65536,
            )

    def test_flawed_stub_finds_predeclared_defects(self):
        record = run_case()
        self.assertEqual(record["outcome"], "success_with_findings")
        self.assertEqual(record["findings_count"], 2)
        self.assertEqual([f["id"] for f in record["findings"]], ["F1", "F2"])
        for finding in record["findings"]:
            for field in ("contract_or_path", "trigger", "consequence",
                          "evidence", "smallest_correction"):
                self.assertTrue(finding[field].strip())

    def test_sound_stub_reports_zero_findings(self):
        record = run_case(scenario="sound")
        self.assertEqual(record["outcome"], "success_zero_findings")
        self.assertEqual(record["findings_count"], 0)

    def test_failed_launch_is_failure(self):
        record = run_case(scenario="fail")
        self.assertEqual(record["outcome"], "failure")
        self.assertEqual(record["error_kind"], "reviewer_exit")
        self.assertNotEqual(record["reviewer_exit_code"], 0)

    def test_timeout_is_distinct_outcome(self):
        record = run_case(scenario="timeout", limit_timeout=2)
        self.assertEqual(record["outcome"], "timeout")

    def test_unavailable_capability_is_not_run(self):
        record = run_case(scenario="unavailable")
        self.assertEqual(record["outcome"], "not_run")

    def test_marker_leak_fails_isolation(self):
        record = run_case(scenario="leak", parent_marker=LEAK_MARKER)
        self.assertEqual(record["outcome"], "failure")
        self.assertTrue(record["marker_leaked"])
        self.assertEqual(record["error_kind"], "isolation_marker_leaked")

    def test_marker_probe_clean_when_not_leaked(self):
        record = run_case(scenario="sound", parent_marker=LEAK_MARKER)
        self.assertEqual(record["outcome"], "success_zero_findings")
        self.assertFalse(record["marker_leaked"])

    def test_external_reviewer_without_marker_is_unverified(self):
        args = argparse_namespace(dict(CASE, scenario="sound"))
        args.stub = None
        args.reviewer_command_json = json.dumps([
            sys.executable, str(runner_mod.STUB), "--scenario", "sound",
        ])
        record = runner_mod.run_one_review(str(REPO), args, case=CASE)
        self.assertEqual(record["outcome"], "unverified")
        self.assertEqual(record["reviewer_mode"], "external")

    def test_oversized_output_is_bounded_and_recorded_sanitized(self):
        record = run_case(scenario="huge", limit_output=65536)
        self.assertEqual(record["outcome"], "failure")
        self.assertEqual(record["error_kind"], "unparsable_output")
        # Raw reviewer output never enters the record.
        self.assertNotIn("stderr", record)
        self.assertNotIn("stdout", record)
        serialized = packet_mod.canonical_record(record)
        self.assertNotIn('"huge"', serialized)

    def test_record_has_frozen_packet_provenance(self):
        record = run_case()
        packet = record["packet"]
        self.assertEqual(packet["packet_hash"], record["packet_hash"])
        self.assertEqual(packet["revisions"]["design_doc"]["path"],
                         CASE["design_doc"])
        self.assertIn("commit", packet["revisions"])
        self.assertEqual(packet["schema_version"], 1)

    def test_disposition_workflow(self):
        with tempfile.TemporaryDirectory() as tmp:
            record_path = Path(tmp) / "record.json"
            record = run_case()
            runner_mod.write_record(str(record_path), record)
            # New records start with an empty disposition list.
            stored = json.loads(record_path.read_text())
            self.assertEqual(stored["dispositions"], [])
            runner_mod.dispose(str(record_path), "F1", "confirmed",
                               "applies to delegation cache")
            stored = json.loads(record_path.read_text())
            self.assertEqual(stored["dispositions"],
                             [{"id": "F1", "status": "confirmed",
                               "note": "applies to delegation cache"}])
            runner_mod.dispose(str(record_path), "F1", "rejected", None)
            stored = json.loads(record_path.read_text())
            self.assertEqual(stored["dispositions"][0]["status"], "rejected")
            with self.assertRaises(ValueError):
                runner_mod.dispose(str(record_path), "F9", "confirmed", None)

    def test_main_requires_explicit_capability(self):
        with tempfile.TemporaryDirectory() as tmp:
            record_path = str(Path(tmp) / "record.json")
            args = argparse_namespace(CASE)
            args.stub = None
            args.record_out = record_path
            code = runner_mod.main([
                "--design-doc", CASE["design_doc"],
                "--revision", "HEAD",
                "--record-out", record_path,
                "--objective", CASE["objective"],
                "--proposed-contract", CASE["proposed_contract"],
            ])
            self.assertEqual(code, 0)
            stored = json.loads(Path(record_path).read_text())
            self.assertEqual(stored["outcome"], "not_run")


if __name__ == "__main__":
    unittest.main()
