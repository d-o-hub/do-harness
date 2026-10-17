"""
report.py — Documentation report and bounded reviewer packet generation.
"""

import hashlib
import json
import os
import sys

from doc_tools.cli_sync import run_sync
from doc_tools.examples import extract_example_blocks, load_fixture_manifest
from doc_tools.impact import analyze_impact, get_diff_hunks_for_file
from doc_tools.inventory import discover_documents
from doc_tools.markdown import MarkdownDocument


def compute_sha256(data_bytes):
    """Computes SHA-256 hex digest of bytes."""
    return hashlib.sha256(data_bytes).hexdigest()


def compute_file_sha256(filepath):
    """Computes SHA-256 hex digest of a file on disk if exists."""
    if not os.path.isfile(filepath):
        return None
    with open(filepath, "rb") as f:
        return compute_sha256(f.read())


import io
import contextlib

def check_generated_section_equality(root_dir):
    """
    Checks if generated sections are in byte-for-byte sync using cli_sync.
    Returns bool indicating equality.
    """
    f = io.StringIO()
    with contextlib.redirect_stdout(f), contextlib.redirect_stderr(f):
        code = run_sync(root_dir, write=False)
    return code == 0


def extract_sections_from_doc(content, section_ids):
    """
    Extracts heading sections from Markdown document text matching section_ids or slugs.
    If section_ids is empty or contains '*', returns full content or all sections.
    """
    lines = content.splitlines()
    doc = MarkdownDocument("temp.md", content)

    # Map anchors/slugs to line ranges
    # Standard Markdown headings match anchors
    matched_ranges = []

    if not section_ids:
        return content

    # Find heading lines and their text
    headings = []
    for i, line in enumerate(lines):
        if line.startswith("#"):
            heading_text = line.lstrip("#").strip()
            from doc_tools.github_slugger import slugify
            slug = slugify(heading_text)
            headings.append({
                "line_no": i,
                "text": heading_text,
                "slug": slug,
            })

    for h_idx, h in enumerate(headings):
        # Check if slug or any requested section ID matches
        match = False
        for sec in section_ids:
            sec_clean = sec.lower().strip()
            if sec_clean == h["slug"] or sec_clean in h["slug"] or sec_clean in h["text"].lower():
                match = True
                break

        if match:
            start_line = h["line_no"]
            end_line = headings[h_idx + 1]["line_no"] if h_idx + 1 < len(headings) else len(lines)
            matched_ranges.append((start_line, end_line))

    if not matched_ranges:
        # Fallback: return first 50 lines if no section match
        return "\n".join(lines[:50])

    extracted_lines = []
    for start, end in matched_ranges:
        extracted_lines.extend(lines[start:end])
        extracted_lines.append("")

    return "\n".join(extracted_lines).strip()


def build_doc_report_and_packet(root_dir, base_rev, head_rev, contracts_path="docs/contracts.toml", max_packet_bytes=100000):
    """
    Builds the complete machine report JSON and companion reviewer packet text.
    """
    root_dir = os.path.abspath(root_dir)
    impact_data = analyze_impact(root_dir, base_rev, head_rev, contracts_path=contracts_path)

    base_commit = impact_data["merge_base"]
    head_commit = impact_data["head_commit"]

    # Mechanical checks with captured stdout/stderr
    f_out = io.StringIO()
    f_err = io.StringIO()
    mechanical_findings = []

    with contextlib.redirect_stdout(f_out), contextlib.redirect_stderr(f_err):
        try:
            generated_equal = check_generated_section_equality(root_dir)
        except Exception as e:
            generated_equal = False
            mechanical_findings.append(f"Generated section equality check error: {e}")

        try:
            from doc_tools.examples import verify_examples
            examples_ok = verify_examples(root_dir)
        except Exception as e:
            examples_ok = False
            mechanical_findings.append(f"Examples verification error: {e}")

        try:
            from docs import LinkChecker
            c_path = os.path.join(root_dir, contracts_path) if not os.path.isabs(contracts_path) else contracts_path
            link_checker = LinkChecker(root_dir=root_dir, contracts_path=c_path, mode="local")
            links_ok = link_checker.check_links()
        except Exception as e:
            links_ok = False
            mechanical_findings.append(f"Link check error: {e}")

    if not generated_equal and "Generated section equality" not in str(mechanical_findings):
        mechanical_findings.append("Generated documentation sections do not match Clap definitions.")
    if not examples_ok and "Examples verification" not in str(mechanical_findings):
        mechanical_findings.append("Documented CLI examples failed verification.")
    if not links_ok and "Link check" not in str(mechanical_findings):
        mechanical_findings.append(f"Link check failed with {len(link_checker.errors)} error(s).")

    # Complete Machine Report (v1)
    report = {
        "version": 1,
        "revisions": {
            "base_rev": base_rev,
            "head_rev": head_rev,
            "merge_base": base_commit,
            "head_commit": head_commit,
        },
        "identities": {
            "contracts_policy_hash": compute_file_sha256(os.path.join(root_dir, contracts_path)),
            "checker_integrity_ok": not impact_data["checker_code_changed"],
            "policy_tampering": impact_data["policy_tampering"],
        },
        "mechanical": {
            "findings": mechanical_findings,
            "generated_section_equality": generated_equal,
            "examples_ok": examples_ok,
            "links_ok": links_ok,
        },
        "impact": {
            "changed_doc_files": impact_data["changed_doc_files"],
            "changed_source_files": impact_data["changed_source_files"],
            "affected_doc_sections": impact_data["affected_doc_sections"],
            "unknown_scope_files": impact_data["unknown_scope_files"],
        },
    }

    # Bounded Reviewer Packet Construction
    packet_blocks = []
    packet_blocks.append("=== DOCS SEMANTIC REVIEW PACKET ===")
    packet_blocks.append(f"Base: {base_commit[:8]} | Head: {head_commit[:8]}")
    packet_blocks.append(f"Mechanical Status: {'PASS' if not mechanical_findings else 'FAIL'}")
    if mechanical_findings:
        packet_blocks.append("Mechanical Diagnostics:")
        for f in mechanical_findings:
            packet_blocks.append(f"  - {f}")

    if impact_data["policy_tampering"]:
        packet_blocks.append("\n⚠️ WARNING: Policy Tampering Detected (docs/contracts.toml modified). Complete manual review required.")

    if impact_data["checker_code_changed"]:
        packet_blocks.append("\n⚠️ WARNING: Documentation Checker / Script Code Changed. Complete manual review required.")

    if impact_data["unknown_scope_files"]:
        packet_blocks.append("\n⚠️ Unmapped Source Changes (Uncertain Scope):")
        for uf in impact_data["unknown_scope_files"]:
            packet_blocks.append(f"  - {uf}")

    # Section 1: Changed / Affected Narrative Sections
    packet_blocks.append("\n--- AFFECTED NARRATIVE SECTIONS ---")

    # Include changed doc files directly
    for doc_file in impact_data["changed_doc_files"]:
        full_doc_path = os.path.join(root_dir, doc_file)
        if os.path.isfile(full_doc_path):
            packet_blocks.append(f"\n[Changed Document: {doc_file}]")
            hunks = get_diff_hunks_for_file(root_dir, base_commit, head_commit, doc_file)
            if hunks:
                packet_blocks.append(hunks)
            else:
                with open(full_doc_path, "r", encoding="utf-8") as f:
                    packet_blocks.append(f.read()[:2000])

    # Include mapped source impact sections
    for doc_path, info in impact_data["affected_doc_sections"].items():
        if doc_path in impact_data["changed_doc_files"]:
            continue  # Already included above as changed doc file
        full_doc_path = os.path.join(root_dir, doc_path)
        if os.path.isfile(full_doc_path):
            packet_blocks.append(f"\n[Mapped Doc: {doc_path} (Sections: {', '.join(info['sections'])})]")
            packet_blocks.append(f"Reasons: {', '.join(info['reasons'])}")
            with open(full_doc_path, "r", encoding="utf-8") as f:
                content = f.read()
            extracted = extract_sections_from_doc(content, info["sections"])
            packet_blocks.append(extracted)

    # Section 2: Relevant Source Context Hunks
    packet_blocks.append("\n--- RELEVANT SOURCE HUNKS ---")
    for src_file in impact_data["changed_source_files"]:
        hunks = get_diff_hunks_for_file(root_dir, base_commit, head_commit, src_file)
        if hunks:
            packet_blocks.append(f"\n[Source Diff: {src_file}]")
            # Limit individual hunk text length
            if len(hunks) > 3000:
                hunks = hunks[:3000] + "\n... [truncated hunk]"
            packet_blocks.append(hunks)

    raw_packet_text = "\n".join(packet_blocks)
    raw_packet_bytes = raw_packet_text.encode("utf-8")

    # Bounding check
    overflow = False
    if len(raw_packet_bytes) > max_packet_bytes:
        overflow = True
        truncated_packet = raw_packet_bytes[:max_packet_bytes].decode("utf-8", errors="ignore")
        truncated_packet += "\n\n⚠️ PACKET OVERFLOW EXCEEDED BUDGET: Fallback to full diff required."
        packet_text = truncated_packet
    else:
        packet_text = raw_packet_text

    report["packet_summary"] = {
        "packet_bytes": len(raw_packet_bytes),
        "bounded_bytes": len(packet_text.encode("utf-8")),
        "overflow": overflow,
    }

    return report, packet_text
