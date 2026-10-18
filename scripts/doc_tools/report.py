"""Versioned documentation-impact report and bounded reviewer packet.

The packet is a companion to code review, never a replacement for it. Local
checks are the only accepted evidence source; this module does not ingest CI
artifacts or infer semantic correctness from generated-text equality.
"""

from __future__ import annotations

import hashlib
import json
import os
import subprocess  # nosec B404
from typing import Any

from doc_tools import acknowledgments, examples, impact
from doc_tools.inventory import discover_documents

REPORT_SCHEMA_VERSION = 1
MAX_PACKET_BYTES = 24_000
_CHECKS = ("sync", "examples", "links")


def _run_check(root: str, check: str) -> dict[str, Any]:
    command = ["python3", "scripts/docs.py", check]
    command.extend(["--check", "--root", root])
    try:
        result = subprocess.run(
            command,
            cwd=root,
            capture_output=True,
            text=True,
            encoding="utf-8",
            errors="replace",
            check=False,
        )  # nosec B603 B607
    except OSError as error:
        return {
            "status": "unavailable",
            "returncode": None,
            "command": command,
            "stdout": "",
            "stderr": str(error),
        }
    return {
        "status": "pass" if result.returncode == 0 else "fail",
        "returncode": result.returncode,
        "command": command,
        "stdout": result.stdout,
        "stderr": result.stderr,
    }


def _collect_verified_example_ids(root: str) -> list[str]:
    contracts_path = os.path.join(root, "docs", "contracts.toml")
    manifest_path = os.path.join(root, "tests", "fixtures", "docs-examples", "manifest.toml")
    inventory = discover_documents(root, contracts_path, mode="local")
    fixtures = examples.load_fixture_manifest(manifest_path)
    verified: set[str] = set()
    for document in inventory["maintained"]:
        full_path = os.path.join(root, document.rel_path)
        if not os.path.isfile(full_path):
            continue
        with open(full_path, encoding="utf-8") as source:
            blocks, errors = examples.extract_example_blocks(document.rel_path, source.read())
        if errors:
            raise ValueError("example checker passed but extraction reported errors")
        for block in blocks:
            marker = block.marker
            if marker.classification == "parser-check-only":
                verified.add(marker.example_id)
            elif marker.classification == "executable":
                fixture = fixtures.get(marker.fixture_id)
                if fixture is None:
                    raise ValueError(f"verified example references missing fixture {marker.fixture_id}")
                platforms = fixture.get("platforms")
                if not platforms or examples.host_platform() in platforms:
                    verified.add(marker.example_id)
    return sorted(verified)


def _result_trusted(result: dict[str, Any], analysis: dict[str, Any], injected: bool) -> bool:
    if result.get("status") != "pass":
        return False
    if not analysis.get("checkout", {}).get("eligible") or not analysis.get("checker", {}).get("unchanged"):
        return False
    if analysis.get("policy", {}).get("changed_at_head"):
        return False
    return not injected or bool(result.get("trusted_for_test", False))



def _packet_content(
    analysis: dict[str, Any],
    checks: dict[str, dict[str, Any]],
    omit_generated: bool,
    full_documents: list[dict[str, str]] | None = None,
    obligations: list[dict[str, Any]] | None = None,
) -> tuple[str, list[str], list[str]]:
    omitted: list[str] = []
    included: list[str] = []
    lines = [
        "Documentation impact packet (mechanical synchronization only; not semantic correctness)",
        f"base={analysis['revisions']['base']}",
        f"head={analysis['revisions']['head']}",
        f"merge_base={analysis['revisions']['merge_base']}",
        f"head_tree={analysis['inputs']['head_tree']}",
        f"policy_sha256={analysis['policy']['sha256']} source={analysis['policy']['source_revision']}",
        f"checker_unchanged={str(analysis['checker']['unchanged']).lower()}",
        "",
        "## Mechanical evidence",
    ]
    for name in _CHECKS:
        result = checks[name]
        lines.append(f"- {name}: {result['status']}; trusted={str(result.get('trusted', False)).lower()}")
        for field in ("stdout", "stderr"):
            value = result.get(field, "").rstrip()
            if value:
                lines.append(f"  {field}: {value}")
    if analysis.get("source_path_reasons"):
        lines.extend(["", "## Source-path reasons"])
        for source in analysis["source_path_reasons"]:
            sections = ", ".join(source.get("sections", [])) or "unknown scope"
            lines.append(f"- {source['path']}: {source['reason']} -> {sections}")
    if obligations:
        lines.extend(["", "## Source-to-document obligations"])
        for obligation in obligations:
            lines.append(
                f"- {obligation['source_path']} -> {obligation['section_id']}: {obligation['status']}"
            )
            if obligation.get("rationale"):
                lines.append(f"  rationale: {obligation['rationale']}")
    if analysis.get("unknown_scope"):
        lines.extend(["", "## Unknown scope (raw review required)"])
        for item in analysis["unknown_scope"]:
            lines.append(f"- {item['path']}: {item['reason']}")
    if full_documents is None:
        lines.extend(["", "## Affected narrative sections"])
        for section in analysis.get("affected_sections", []):
            section_id = section["section_id"]
            if omit_generated and section.get("generated_only"):
                omitted.extend(section.get("generated_region_ids", []))
                continue
            included.append(section_id)
            lines.extend([f"### {section_id}", section.get("text", "")])
        if omitted:
            lines.extend(["", "## Mechanically synchronized generated regions"])
            lines.append("- omitted after exact local sync check: " + ", ".join(sorted(set(omitted))))
    else:
        lines.extend(["", "## Full-document input"])
        for document in full_documents:
            lines.extend([f"### {document['path']} ({document['revision']})", document["content"]])
    source_diff = analysis.get("source_diff", "")
    if source_diff:
        lines.extend(["", "## Relevant source hunks", "```diff", source_diff.rstrip(), "```"])
    lines.extend(
        [
            "",
            "## Review boundary",
            "This packet supplements the complete code review. It does not remove code units from `do-harness pr review`.",
            "Generated equality proves synchronization only, not semantic correctness.",
        ]
    )
    return "\n".join(lines).rstrip() + "\n", sorted(set(omitted)), included


def _reported_sections(analysis: dict[str, Any]) -> list[dict[str, Any]]:
    sections = sorted(analysis.get("affected_sections", []), key=lambda item: item["section_id"])
    return [
        {
            "section_id": section["section_id"],
            "path": section["path"],
            "heading": section["heading"],
            "line_start": section["line_start"],
            "line_end": section["line_end"],
            "generated_only": section["generated_only"],
            "generated_region_ids": sorted(set(section["generated_region_ids"])),
            "source_paths": sorted(set(section["source_paths"])),
            "text_sha256": hashlib.sha256(section["text"].encode("utf-8")).hexdigest(),
        }
        for section in sections
    ]


def build_report(
    root: str,
    analysis: dict[str, Any],
    check_results: dict[str, dict[str, Any]] | None = None,
    acknowledgments_path: str | None = None,
) -> dict[str, Any]:
    """Build a complete machine report; injected check results are test-only."""
    root = os.path.abspath(root)
    acknowledgment_data = acknowledgments.evaluate(root, analysis, acknowledgments_path)
    injected = check_results is not None
    if check_results is None:
        if analysis.get("checkout", {}).get("eligible"):
            checks = {name: _run_check(root, name) for name in _CHECKS}
        else:
            reason = "head is not the exact clean checkout; local checks were not run"
            checks = {
                name: {"status": "not_run", "returncode": None, "command": [], "stdout": "", "stderr": reason}
                for name in _CHECKS
            }
    else:
        checks = {}
        for name in _CHECKS:
            result = dict(check_results.get(name, {}))
            result.setdefault("status", "not_run")
            result.setdefault("returncode", None)
            result.setdefault("command", [])
            result.setdefault("stdout", "")
            result.setdefault("stderr", "")
            checks[name] = result
    for name in _CHECKS:
        checks[name]["trusted"] = _result_trusted(checks[name], analysis, injected)
        checks[name]["origin"] = "test-injected" if injected else "local-process"

    if checks["examples"]["trusted"]:
        try:
            verified_example_ids = _collect_verified_example_ids(root)
        except Exception as error:  # a checker pass without a complete ID inventory is not evidence
            checks["examples"]["trusted"] = False
            checks["examples"]["stderr"] = (checks["examples"].get("stderr", "") + f"\nID inventory failed: {error}").strip()
            verified_example_ids = []
    else:
        verified_example_ids = []

    sync = checks["sync"]
    verified_regions = sorted(
        {
            region_id
            for section in analysis.get("affected_sections", [])
            for region_id in section.get("generated_region_ids", [])
        }
    ) if sync["trusted"] else []
    generated_equality = {
        "status": sync["status"],
        "trusted": sync["trusted"],
        "eligible_for_omission": bool(sync["trusted"]),
        "verified_region_ids": verified_regions,
        "scope": "byte-for-byte generated synchronization only; not semantic correctness",
    }
    mechanical_findings = [
        {
            "check": name,
            "status": checks[name]["status"],
            "returncode": checks[name].get("returncode"),
            "diagnostics": "\n".join(value for value in (checks[name].get("stdout", "").rstrip(), checks[name].get("stderr", "").rstrip()) if value),
        }
        for name in _CHECKS
        if checks[name]["status"] != "pass"
    ]
    omit_generated = generated_equality["eligible_for_omission"]
    packet_text, omitted, included = _packet_content(
        analysis, checks, omit_generated, obligations=acknowledgment_data["obligations"]
    )
    raw_text, _, _ = _packet_content(
        analysis,
        checks,
        omit_generated=False,
        full_documents=impact.full_document_inputs(root, analysis),
        obligations=acknowledgment_data["obligations"],
    )
    raw_equivalent_bytes = len(raw_text.encode("utf-8"))
    packet_bytes = len(packet_text.encode("utf-8"))
    complete_checks = all(checks[name]["trusted"] for name in _CHECKS)
    packet_status = "complete"
    fallback_reasons = []
    if analysis.get("unknown_scope"):
        packet_status = "fallback_required"
        fallback_reasons.append("unknown source/document scope")
    if not complete_checks:
        packet_status = "fallback_required"
        fallback_reasons.append("local mechanical evidence is incomplete or untrusted")
    if packet_bytes > MAX_PACKET_BYTES:
        packet_status = "overflow"
        fallback_reasons.append("complete packet exceeds the byte ceiling")
        packet_content: str | None = None
    else:
        packet_content = packet_text
    packet = {
        "status": packet_status,
        "complete": packet_status == "complete",
        "fallback_required": packet_status != "complete",
        "fallback_reasons": fallback_reasons,
        "max_bytes": MAX_PACKET_BYTES,
        "raw_equivalent_bytes": raw_equivalent_bytes,
        "bytes": packet_bytes,
        "content": packet_content,
        "included_section_ids": included,
        "omitted_generated_region_ids": omitted,
    }
    artifact_refs = []
    for name in _CHECKS:
        result = checks[name]
        artifact_refs.append(
            {
                "kind": "local-check-result",
                "check": name,
                "source": result["origin"],
                "revision": analysis["revisions"]["head"],
                "status": result["status"],
                "checker_sha256": analysis["checker"].get("head_sha256"),
                "stdout_sha256": hashlib.sha256(result.get("stdout", "").encode("utf-8")).hexdigest(),
                "stderr_sha256": hashlib.sha256(result.get("stderr", "").encode("utf-8")).hexdigest(),
            }
        )
    return {
        "schema_version": REPORT_SCHEMA_VERSION,
        "revisions": analysis["revisions"],
        "inputs": analysis["inputs"],
        "policy": analysis["policy"],
        "checker": analysis["checker"],
        "checkout": analysis["checkout"],
        "changes": analysis["changes"],
        "mechanical_checks": checks,
        "mechanical_findings": mechanical_findings,
        "generated_equality": generated_equality,
        "verified_example_ids": verified_example_ids,
        "affected_sections": _reported_sections(analysis),
        "source_section_obligations": acknowledgment_data["obligations"],
        "human_acknowledgments": {
            "source": acknowledgment_data["source"],
            "sha256": acknowledgment_data["sha256"],
            "entries": acknowledgment_data["entries"],
        },
        "source_path_reasons": analysis.get("source_path_reasons", []),
        "unknown_scope": analysis.get("unknown_scope", []),
        "reviewable_paths": analysis.get("reviewable_paths", []),
        "artifact_refs": artifact_refs,
        "external_artifacts_accepted": False,
        "proof_scope": "mechanical synchronization and executable example checks only; semantic correctness remains a human review obligation",
        "reviewer_packet": packet,
    }


def serialize_report(report: dict[str, Any]) -> str:
    """Serialize a stable JSON report suitable for CI artifacts and review tools."""
    return json.dumps(report, ensure_ascii=True, indent=2, sort_keys=True) + "\n"
