#!/usr/bin/env python3
"""Measure packet/effective-input bytes and deterministic process/tool calls.

The six local Git fixtures exercise generated-only, narrative-only, mixed,
rename/delete, policy-tampering, and unmapped cases. Fallbacks count raw
full-document input. No network, model, external artifact, or token estimate
is used.
"""

import json
import sys
from pathlib import Path
from unittest import mock

ROOT = Path(__file__).resolve().parents[1]
SCRIPTS = ROOT / "scripts"
sys.path.insert(0, str(SCRIPTS))
sys.path.insert(0, str(SCRIPTS / "tests"))

from doc_tools import git_helpers, impact, report  # noqa: E402
from test_doc_impact import ImpactRepo, PASSING_CHECKS  # noqa: E402

def _generated_only(repo):
    repo._write("crates/do-harness/src/cli.rs", "pub fn command() { println!(\"new\"); }\n")
    readme = repo._read("README.md").replace("Command caveat.\n", "")
    repo._write("README.md", readme.replace("old command row", "new command row"))


def _narrative_only(repo):
    adoption = repo._read("docs/adoption.md").replace("CI text.", "Updated CI text.")
    repo._write("docs/adoption.md", adoption)


def _mixed(repo):
    repo._write("crates/do-harness/src/cli.rs", "pub fn command() { println!(\"new\"); }\n")
    readme = repo._read("README.md").replace("old command row", "new command row")
    repo._write("README.md", readme.replace("Command caveat.", "Updated command caveat."))


def _rename_delete(repo):
    repo._git("mv", "docs/old guide.md", "docs/new guide.md")
    (repo.root / "docs/delete-me.md").unlink()


def _policy_tampering(repo):
    repo._write("crates/do-harness/src/cli.rs", "pub fn command() { println!(\"new\"); }\n")
    policy = repo._read("docs/contracts.toml").replace(
        "CLI definitions drive command reference sections.",
        "Head policy attempts to hide this source.",
    )
    repo._write("docs/contracts.toml", policy)


def _unmapped(repo):
    repo._write("crates/do-harness/src/unmapped.rs", "pub fn changed() {}\n")


CASES = (
    ("generated-only", _generated_only),
    ("narrative-only", _narrative_only),
    ("mixed", _mixed),
    ("renamed-deleted", _rename_delete),
    ("policy-tampering", _policy_tampering),
    ("unmapped", _unmapped),
)


def _measure(name, mutate):
    repo = ImpactRepo()
    try:
        mutate(repo)
        head = repo.finish()
        original_run = git_helpers.subprocess.run
        git_processes = 0

        def counted_run(*args, **kwargs):
            nonlocal git_processes
            git_processes += 1
            return original_run(*args, **kwargs)

        with mock.patch.object(git_helpers.subprocess, "run", side_effect=counted_run):
            analysis = impact.analyze(str(repo.root), repo.base, head)
            result = report.build_report(str(repo.root), analysis, PASSING_CHECKS)
        packet = result["reviewer_packet"]
        packet_bytes = packet["bytes"]
        raw_bytes = packet["raw_equivalent_bytes"]
        packet_consumed = not packet["fallback_required"] and packet_bytes < raw_bytes
        effective_input_bytes = packet_bytes if packet_consumed else raw_bytes
        return {
            "case": name,
            "packet_status": packet["status"],
            "packet_bytes": packet_bytes,
            "full_document_input_bytes": raw_bytes,
            "candidate_reduction_bytes": raw_bytes - packet_bytes,
            "effective_review_input_bytes": effective_input_bytes,
            "effective_reduction_bytes": raw_bytes - effective_input_bytes,
            "packet_consumed": packet_consumed,
            "measured_git_process_calls": git_processes,
            "production_local_check_invocations_per_report": len(report._CHECKS),
            "benchmark_uses_injected_check_results": True,
            "llm_or_api_calls": 0,
            "fallback_required": packet["fallback_required"],
        }
    finally:
        repo.close()


def main():
    cases = [_measure(name, mutate) for name, mutate in CASES]
    packet_bytes = sum(case["packet_bytes"] for case in cases)
    raw_bytes = sum(case["full_document_input_bytes"] for case in cases)
    effective_input_bytes = sum(case["effective_review_input_bytes"] for case in cases)
    report_data = {
        "schema_version": 2,
        "tool_call_definition": (
            "measured Git subprocesses used by the analyzer; production local checks per report "
            "are counted but not executed by this fixture benchmark; effective input counts the "
            "packet only when complete and strictly smaller than equivalent full-document input; "
            "otherwise it counts the raw full-document input; no LLM/API calls"
        ),
        "tokenizer": None,
        "cases": cases,
        "summary": {
            "cases": len(cases),
            "packet_bytes": packet_bytes,
            "candidate_reduction_bytes": raw_bytes - packet_bytes,
            "full_document_input_bytes": raw_bytes,
            "effective_review_input_bytes": effective_input_bytes,
            "effective_reduction_bytes": raw_bytes - effective_input_bytes,
            "effective_reduction_percent": (
                round((raw_bytes - effective_input_bytes) * 100 / raw_bytes, 2)
                if raw_bytes
                else 0.0
            ),
            "packets_consumed": sum(case["packet_consumed"] for case in cases),
            "measured_git_process_calls": sum(case["measured_git_process_calls"] for case in cases),
            "production_local_check_invocations_per_report": len(report._CHECKS),
            "llm_or_api_calls": 0,
        },
    }
    print(json.dumps(report_data, ensure_ascii=True, indent=2, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
