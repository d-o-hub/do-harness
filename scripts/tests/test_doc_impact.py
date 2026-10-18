#!/usr/bin/env python3
"""Can-fail contracts for NUL-safe documentation impact analysis and packets."""

import contextlib
import hashlib
import io
import json
import os
import subprocess  # nosec B404
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock

SCRIPTS = Path(__file__).resolve().parents[1]
REPO_ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(SCRIPTS))

from doc_tools import impact, report  # noqa: E402
from doc_tools.sections import extract_sections  # noqa: E402
from docs import main as docs_main  # noqa: E402

POLICY = '''version = 1

[[rules]]
path = "README.md"
classification = "maintained"
reason = "Root guide."

[[rules]]
path = "AGENTS.md"
classification = "maintained"
reason = "Repository instructions."

[[rules]]
glob = "docs/*.md"
classification = "maintained"
reason = "Maintained docs."

[[impact_map]]
path = "crates/do-harness/src/cli.rs"
sections = ["README.md#commands", "docs/cli.md#subcommands"]
reason = "CLI definitions drive command reference sections."

[[impact_map]]
path = "do-harness.toml"
sections = ["README.md#ci", "docs/adoption.md#ci-snippets", "AGENTS.md#4-cli-tool-protocol--guardrails"]
reason = "Sensor and workflow policy drives verification guidance."

[[impact_map]]
glob = "crates/do-harness/src/**"
sections = []
reason = "Catch-all: unmapped Rust source needs raw review."
'''

PASSING_CHECKS = {
    name: {"status": "pass", "returncode": 0, "stdout": "OK", "stderr": "", "trusted_for_test": True}
    for name in ("sync", "examples", "links")
}


class ImpactRepo:
    """A real temporary Git repository exercising the production analyzer."""

    def __init__(self):
        self._temp = tempfile.TemporaryDirectory()
        self.root = Path(self._temp.name)
        self._git("init", "-q")
        self._git("config", "user.name", "Impact Test")
        self._git("config", "user.email", "impact@example.invalid")
        self._write("docs/contracts.toml", POLICY)
        self._write(
            "README.md",
            "# Guide\n\n## Background\n"
            + ("Repository overview and operational context. " * 80)
            + "\n\n## Commands\n"
            + "<!-- The command table is generated. -->\n"
            + "Command caveat.\n"
            + "<!-- BEGIN GENERATED: cli-commands -->\nold command row\n"
            + "<!-- END GENERATED: cli-commands -->\n\n"
        )
        self._write(
            "docs/cli.md",
            "# CLI reference\n\n## Overview\n"
            + ("Command behavior and supported workflows. " * 80)
            + "\n\n## Subcommands\n### verify\n"
            + "<!-- BEGIN GENERATED: cli-contract verify -->\nold usage\n"
            + "<!-- END GENERATED: cli-contract verify -->\n",
        )
        self._write(
            "docs/adoption.md",
            "# Adoption\n\n## Initialize the repository\n"
            + ("Initialization context and setup details. " * 60)
            + "\n\n## Configure sensors for your stack\nSensor text.\n\n"
            + "## The agent loop\nAgent text.\n\n## CI snippets\nCI text.\n",
        )
        self._write("docs/preface.md", "Repository overview.\n\n# Details\nDetailed text.\n")
        self._write("AGENTS.md", "# Instructions\n\n## 4. CLI Tool Protocol & Guardrails\nWorkflow text.\n")
        self._write("docs/old guide.md", "# Old guide\n\n## Historical instructions\nOld file.\n")
        self._write("docs/delete-me.md", "# Delete me\n\n## Removal section\nOld section.\n")
        self._write("crates/do-harness/src/cli.rs", "pub fn command() { println!(\"old\"); }\n")
        self._write("do-harness.toml", "[signal-sets]\nverification = []\n")
        self._write("scripts/docs.py", "trusted checker v1\n")
        self._write("scripts/doc_tools/cli_sync.py", "trusted sync v1\n")
        self._write("scripts/doc_tools/examples.py", "trusted examples v1\n")
        self._write("scripts/doc_tools/inventory.py", "trusted inventory v1\n")
        self._write("scripts/doc_tools/markdown.py", "trusted parser v1\n")
        self._write("scripts/doc_tools/impact.py", "trusted impact v1\n")
        self._write("scripts/doc_tools/report.py", "trusted report v1\n")
        self._write("scripts/test_doc_tools.py", "trusted assertions v1\n")
        self._write("scripts/tests/test_examples.py", "trusted example assertions v1\n")
        self._write("scripts/requirements-docs.txt", "markdown-it-py==3.0.0\n")
        self._write("crates/do-harness/src/docs_generation/render.rs", "trusted generator v1\n")
        self._write("tests/fixtures/cli-contract.json", "{}\n")
        self._write("tests/fixtures/docs-examples/manifest.toml", "version = 1\n[fixtures]\n")
        self.base = self._commit("base")

    def close(self):
        self._temp.cleanup()

    def _git(self, *args):
        return subprocess.run(  # nosec B603 B607
            ["git", *args], cwd=self.root, check=True, capture_output=True, text=True
        ).stdout.strip()

    def _write(self, rel_path, text):
        path = self.root / rel_path
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text, encoding="utf-8")

    def _read(self, rel_path):
        return (self.root / rel_path).read_text(encoding="utf-8")

    def _commit(self, subject):
        self._git("add", "-A")
        self._git("commit", "-q", "-m", subject)
        return self._git("rev-parse", "HEAD")

    def finish(self):
        return self._commit("head")

    def report(self, checks=None):
        head = self.finish()
        analysis = impact.analyze(str(self.root), self.base, head)
        result = report.build_report(str(self.root), analysis, checks or PASSING_CHECKS)
        return analysis, result

    def acknowledgment_entry(self, analysis, source_path, section_id, rationale):
        head = analysis["revisions"]["head"]
        merge_base = analysis["revisions"]["merge_base"]
        document_path = section_id.split("#", 1)[0]

        def blob(revision, path):
            return subprocess.run(  # nosec B603 B607
                ["git", "-C", str(self.root), "cat-file", "blob", f"{revision}:{path}"],
                capture_output=True,
                check=True,
            ).stdout

        def section_digest(document_blob):
            text = document_blob.decode("utf-8")
            section = next(
                item for item in extract_sections(document_path, text) if item.section_id == section_id
            )
            return hashlib.sha256(section.text.encode("utf-8")).hexdigest()

        source_blob = blob(head, source_path)
        head_document = blob(head, document_path)
        base_document = blob(merge_base, document_path)
        return {
            "source_path": source_path,
            "source_revision": head,
            "source_sha256": hashlib.sha256(source_blob).hexdigest(),
            "section_id": section_id,
            "document_revision": head,
            "document_sha256": hashlib.sha256(head_document).hexdigest(),
            "document_section_sha256": section_digest(head_document),
            "base_document_revision": merge_base,
            "base_document_sha256": hashlib.sha256(base_document).hexdigest(),
            "base_document_section_sha256": section_digest(base_document),
            "rationale": rationale,
        }


class TestImpactAnalysis(unittest.TestCase):
    def setUp(self):
        self.repo = ImpactRepo()

    def tearDown(self):
        self.repo.close()

    def test_repository_impact_map_targets_maintained_headings(self):
        policy_path = REPO_ROOT / "docs" / "contracts.toml"
        document_rules, impact_rules, _ = impact._parse_policy(policy_path.read_bytes())
        for rule in impact_rules:
            for section_id in rule.sections:
                path = section_id.split("#", 1)[0]
                classification = impact._classify_document(path, document_rules)
                self.assertIn(classification, ("maintained", "canonical_alias"), section_id)
                document = (REPO_ROOT / path).read_text(encoding="utf-8")
                actual_ids = {section.section_id for section in extract_sections(path, document)}
                self.assertIn(section_id, actual_ids, f"{rule.pattern} maps to missing heading {section_id}")

    def test_generated_only_is_omitted_only_after_trusted_exact_sync(self):
        self.repo._write("crates/do-harness/src/cli.rs", "pub fn command() { println!(\"new\"); }\n")
        readme = self.repo._read("README.md").replace("old command row", "new command row")
        readme = readme.replace("Command caveat.\n", "")
        self.repo._write("README.md", readme)
        analysis, result = self.repo.report()
        packet = result["reviewer_packet"]
        self.assertTrue(result["generated_equality"]["eligible_for_omission"])
        self.assertIn("cli-commands", packet["omitted_generated_region_ids"])
        self.assertNotIn("README.md#commands", packet["included_section_ids"])
        self.assertNotIn("new command row", packet["content"])
        self.assertLess(packet["bytes"], packet["raw_equivalent_bytes"])
        self.assertEqual(analysis["source_path_reasons"][0]["sections"][0], "README.md#commands")

    def test_mixed_generated_and_narrative_section_stays_visible(self):
        self.repo._write("crates/do-harness/src/cli.rs", "pub fn command() { println!(\"new\"); }\n")
        readme = self.repo._read("README.md").replace("old command row", "new command row")
        readme = readme.replace("Command caveat.", "Updated command caveat.")
        self.repo._write("README.md", readme)
        _, result = self.repo.report()
        packet = result["reviewer_packet"]
        self.assertNotIn("cli-commands", packet["omitted_generated_region_ids"])
        self.assertIn("README.md#commands", packet["included_section_ids"])
        self.assertIn("new command row", packet["content"])
        self.assertIn("Updated command caveat.", packet["content"])

    def test_narrative_change_selects_its_heading(self):
        adoption = self.repo._read("docs/adoption.md").replace("CI text.", "Updated CI text.")
        self.repo._write("docs/adoption.md", adoption)
        _, result = self.repo.report()
        self.assertIn("docs/adoption.md#ci-snippets", [section["section_id"] for section in result["affected_sections"]])
        self.assertIn("Updated CI text.", result["reviewer_packet"]["content"])

    def test_unchanged_mapped_prose_is_a_rationale_obligation(self):
        self.repo._write("crates/do-harness/src/cli.rs", "pub fn command() { println!(\"new\"); }\n")
        head = self.repo.finish()
        analysis = impact.analyze(str(self.repo.root), self.repo.base, head)
        result = report.build_report(str(self.repo.root), analysis, PASSING_CHECKS)
        obligation = next(
            item
            for item in result["source_section_obligations"]
            if item["section_id"] == "README.md#commands"
        )
        self.assertEqual(obligation["status"], "needs_human_rationale")
        self.assertTrue(obligation["prose_unchanged"])
        self.assertEqual(result["reviewer_packet"]["status"], "complete")
        self.assertIn("needs_human_rationale", result["reviewer_packet"]["content"])

    def test_exact_revision_rationale_is_accepted(self):
        source_path = "crates/do-harness/src/cli.rs"
        section_id = "README.md#commands"
        self.repo._write(source_path, "pub fn command() { println!(\"new\"); }\n")
        head = self.repo.finish()
        analysis = impact.analyze(str(self.repo.root), self.repo.base, head)
        entry = self.repo.acknowledgment_entry(
            analysis,
            source_path,
            section_id,
            "The parser refactor leaves the documented command contract unchanged.",
        )
        with tempfile.TemporaryDirectory() as temp:
            path = Path(temp) / "acknowledgments.json"
            path.write_text(json.dumps({"schema_version": 1, "acknowledgments": [entry]}), encoding="utf-8")
            result = report.build_report(str(self.repo.root), analysis, PASSING_CHECKS, str(path))
        obligation = next(
            item for item in result["source_section_obligations"] if item["section_id"] == section_id
        )
        self.assertEqual(obligation["status"], "acknowledged")
        self.assertIn(entry["rationale"], result["reviewer_packet"]["content"])
        self.assertEqual(result["human_acknowledgments"]["source"], "caller-supplied-file")

    def test_stale_rationale_is_rejected(self):
        source_path = "crates/do-harness/src/cli.rs"
        section_id = "README.md#commands"
        self.repo._write(source_path, "pub fn command() { println!(\"new\"); }\n")
        head = self.repo.finish()
        analysis = impact.analyze(str(self.repo.root), self.repo.base, head)
        entry = self.repo.acknowledgment_entry(analysis, source_path, section_id, "No wording change.")
        entry["source_sha256"] = "0" * 64
        with tempfile.TemporaryDirectory() as temp:
            path = Path(temp) / "stale.json"
            path.write_text(json.dumps({"schema_version": 1, "acknowledgments": [entry]}), encoding="utf-8")
            with self.assertRaises(report.acknowledgments.AcknowledgmentError):
                report.build_report(str(self.repo.root), analysis, PASSING_CHECKS, str(path))

    def test_changed_prose_cannot_be_acknowledged_as_unchanged(self):
        source_path = "crates/do-harness/src/cli.rs"
        section_id = "README.md#commands"
        self.repo._write(source_path, "pub fn command() { println!(\"new\"); }\n")
        self.repo._write("README.md", self.repo._read("README.md").replace("Command caveat.", "Updated caveat."))
        head = self.repo.finish()
        analysis = impact.analyze(str(self.repo.root), self.repo.base, head)
        entry = self.repo.acknowledgment_entry(analysis, source_path, section_id, "No wording change.")
        with tempfile.TemporaryDirectory() as temp:
            path = Path(temp) / "changed.json"
            path.write_text(json.dumps({"schema_version": 1, "acknowledgments": [entry]}), encoding="utf-8")
            with self.assertRaises(report.acknowledgments.AcknowledgmentError):
                report.build_report(str(self.repo.root), analysis, PASSING_CHECKS, str(path))

    def test_preamble_change_is_assigned_to_top_section(self):
        preface = self.repo._read("docs/preface.md").replace("Repository overview.", "Updated overview.")
        self.repo._write("docs/preface.md", preface)
        _, result = self.repo.report()
        self.assertIn("docs/preface.md#top", [section["section_id"] for section in result["affected_sections"]])
        self.assertIn("Updated overview.", result["reviewer_packet"]["content"])

    def test_checker_change_prevents_generated_omission(self):
        self.repo._write("crates/do-harness/src/cli.rs", "pub fn command() { println!(\"new\"); }\n")
        readme = self.repo._read("README.md").replace("old command row", "new command row")
        self.repo._write("README.md", readme.replace("Command caveat.\n", ""))
        self.repo._write("scripts/doc_tools/cli_sync.py", "changed sync checker\n")
        analysis, result = self.repo.report()
        self.assertFalse(analysis["checker"]["unchanged"])
        self.assertFalse(result["generated_equality"]["eligible_for_omission"])
        self.assertIn("new command row", result["reviewer_packet"]["content"])

    def test_merge_base_policy_survives_head_policy_tampering(self):
        self.repo._write("crates/do-harness/src/cli.rs", "pub fn command() { println!(\"new\"); }\n")
        policy = self.repo._read("docs/contracts.toml").replace(
            "CLI definitions drive command reference sections.",
            "Head policy attempts to hide this source.",
        )
        self.repo._write("docs/contracts.toml", policy)
        analysis, result = self.repo.report()
        cli = next(item for item in analysis["source_path_reasons"] if item["path"].endswith("cli.rs"))
        self.assertTrue(analysis["policy"]["changed_at_head"])
        self.assertEqual(cli["reason"], "CLI definitions drive command reference sections.")
        self.assertIn("docs/contracts.toml", result["reviewable_paths"])
        self.assertFalse(result["mechanical_checks"]["sync"]["trusted"])
        self.assertFalse(result["generated_equality"]["eligible_for_omission"])
        self.assertIn("old command row", result["reviewer_packet"]["content"])

    def test_unmapped_source_path_forces_raw_fallback(self):
        self.repo._write("crates/do-harness/src/unmapped.rs", "pub fn changed() {}\n")
        analysis, result = self.repo.report()
        self.assertTrue(any(item["path"].endswith("unmapped.rs") for item in analysis["unknown_scope"]))
        self.assertEqual(result["reviewer_packet"]["status"], "fallback_required")
        self.assertTrue(result["reviewer_packet"]["content"])

    def test_relevant_source_deletion_forces_raw_fallback(self):
        path = "crates/do-harness/src/cli.rs"
        (self.repo.root / path).unlink()
        analysis, result = self.repo.report()
        self.assertTrue(
            any(item["path"] == path and "deletion" in item["reason"] for item in analysis["unknown_scope"])
        )
        self.assertIn(path, analysis["source_diff"])
        self.assertEqual(result["reviewer_packet"]["status"], "fallback_required")

    def test_unclassified_input_is_included_and_forces_raw_fallback(self):
        self.repo._write("scripts/other_tool.py", "changed()\n")
        analysis, result = self.repo.report()
        self.assertIn("scripts/other_tool.py", analysis["reviewable_paths"])
        self.assertIn("scripts/other_tool.py", analysis["source_diff"])
        self.assertTrue(any(item["path"] == "scripts/other_tool.py" for item in analysis["unknown_scope"]))
        self.assertEqual(result["reviewer_packet"]["status"], "fallback_required")

    def test_cargo_lock_change_invalidates_checker_identity(self):
        self.repo._write("crates/do-harness/src/cli.rs", "pub fn command() { println!(\"new\"); }\n")
        readme = self.repo._read("README.md").replace("old command row", "new command row")
        self.repo._write("README.md", readme.replace("Command caveat.\n", ""))
        self.repo._write("Cargo.lock", "version = 4\n")
        analysis, result = self.repo.report()
        self.assertIn("Cargo.lock", analysis["checker"]["changed_paths"])
        self.assertFalse(result["generated_equality"]["eligible_for_omission"])
        self.assertIn("new command row", result["reviewer_packet"]["content"])

    def test_rename_and_deletion_keep_both_paths(self):
        self.repo._git("mv", "docs/old guide.md", "docs/new guide.md")
        (self.repo.root / "docs/delete-me.md").unlink()
        analysis, result = self.repo.report()
        statuses = analysis["changes"]
        self.assertTrue(
            any(
                item["status"].startswith("R")
                and item["old_path"] == "docs/old guide.md"
                and item["new_path"] == "docs/new guide.md"
                for item in statuses
            )
        )
        self.assertTrue(
            any(
                item["status"] == "D"
                and item["old_path"] == "docs/delete-me.md"
                and item["new_path"] is None
                for item in statuses
            )
        )
        self.assertIn("docs/old guide.md", analysis["changed_paths"])
        self.assertIn("docs/new guide.md", analysis["changed_paths"])
        self.assertIn("docs/delete-me.md", analysis["changed_paths"])
        self.assertTrue(result["reviewer_packet"]["fallback_required"])

    def test_unresolvable_revision_is_an_error_not_empty_diff(self):
        with self.assertRaises(impact.ImpactError):
            impact.analyze(str(self.repo.root), self.repo.base, "missing-revision")

    def test_hidden_index_worktree_entry_prevents_local_evidence(self):
        self.repo._git("update-index", "--assume-unchanged", "scripts/docs.py")
        self.repo._write("scripts/docs.py", "modified outside the index\n")
        analysis = impact.analyze(str(self.repo.root), self.repo.base, self.repo.base)
        result = report.build_report(str(self.repo.root), analysis, PASSING_CHECKS)
        self.assertTrue(analysis["checkout"]["index_has_hidden_worktree_entries"])
        self.assertFalse(analysis["checkout"]["eligible"])
        self.assertFalse(result["mechanical_checks"]["sync"]["trusted"])

    def test_overflow_has_no_truncated_packet(self):
        self.repo._write("README.md", self.repo._read("README.md").replace("Command caveat.", "x" * 200))
        head = self.repo.finish()
        analysis = impact.analyze(str(self.repo.root), self.repo.base, head)
        with mock.patch.object(report, "MAX_PACKET_BYTES", 64):
            result = report.build_report(str(self.repo.root), analysis, PASSING_CHECKS)
        packet = result["reviewer_packet"]
        self.assertEqual(packet["status"], "overflow")
        self.assertIsNone(packet["content"])
        self.assertGreater(packet["bytes"], packet["max_bytes"])
        self.assertTrue(packet["fallback_required"])

    def test_cli_emits_machine_report_for_a_noncurrent_head(self):
        self.repo._write("crates/do-harness/src/cli.rs", 'pub fn command() { println!("new"); }' + os.linesep)
        head = self.repo.finish()
        self.repo._git("checkout", "--detach", self.repo.base)
        with tempfile.TemporaryDirectory() as temp:
            ack_path = Path(temp) / "acknowledgments.json"
            ack_path.write_text(json.dumps({"schema_version": 1, "acknowledgments": []}), encoding="utf-8")
            # Drive the command entry in-process: the packaged CLI is the same
            # code path, and a spawned interpreter adds nothing but a Codacy
            # `subprocess` finding and a second environment to keep clean.
            argv = [
                "docs.py",
                "impact",
                "--base",
                self.repo.base,
                "--head",
                head,
                "--format",
                "json",
                "--root",
                str(self.repo.root),
                "--acknowledgments",
                str(ack_path),
            ]
            output = io.StringIO()
            with mock.patch.object(sys, "argv", argv):
                with contextlib.redirect_stdout(output):
                    with self.assertRaises(SystemExit) as raised:
                        docs_main()
        self.assertEqual(raised.exception.code, 0)
        report_data = json.loads(output.getvalue())
        self.assertEqual(report_data["schema_version"], 1)
        self.assertEqual(report_data["revisions"]["head"], head)
        self.assertFalse(report_data["reviewer_packet"]["complete"])
        self.assertEqual(report_data["human_acknowledgments"]["source"], "caller-supplied-file")



def main():
    unittest.main()


if __name__ == "__main__":
    main()
