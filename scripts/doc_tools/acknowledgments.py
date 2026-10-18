"""Revision-bound human rationales for unchanged mapped prose."""

from __future__ import annotations

import hashlib
import json
from typing import Any

from doc_tools import impact
from doc_tools.sections import Section, extract_sections

ACKNOWLEDGMENT_SCHEMA_VERSION = 1
ACKNOWLEDGMENT_FIELDS = frozenset(
    {
        "source_path",
        "source_revision",
        "source_sha256",
        "section_id",
        "document_revision",
        "document_sha256",
        "document_section_sha256",
        "base_document_revision",
        "base_document_sha256",
        "base_document_section_sha256",
        "rationale",
    }
)


class AcknowledgmentError(ValueError):
    """Raised when a rationale is malformed, stale, or does not resolve."""


def _section_state(root: str, revision: str, section_id: str) -> tuple[Section | None, bytes | None]:
    path = section_id.split("#", 1)[0]
    raw = impact._read_blob(root, revision, path)
    if raw is None:
        return None, None
    try:
        sections = extract_sections(path, raw.decode("utf-8"))
    except (UnicodeDecodeError, ValueError, RuntimeError):
        return None, raw
    section = next((item for item in sections if item.section_id == section_id), None)
    return section, raw


def source_section_obligations(root: str, analysis: dict[str, Any]) -> list[dict[str, Any]]:
    """Resolve mapped source/doc identities from Git objects, not report claims."""
    revisions = analysis["revisions"]
    head, merge_base = revisions["head"], revisions["merge_base"]
    obligations = []
    seen: set[tuple[str, str]] = set()
    for source in analysis.get("source_path_reasons", []):
        source_path = source["path"]
        for section_id in source.get("sections", []):
            key = (source_path, section_id)
            if key in seen:
                continue
            seen.add(key)
            source_blob = impact._read_blob(root, head, source_path)
            base_section, base_document_blob = _section_state(root, merge_base, section_id)
            head_section, head_document_blob = _section_state(root, head, section_id)
            base_section_sha = (
                hashlib.sha256(base_section.text.encode("utf-8")).hexdigest()
                if base_section is not None
                else None
            )
            head_section_sha = (
                hashlib.sha256(head_section.text.encode("utf-8")).hexdigest()
                if head_section is not None
                else None
            )
            prose_unchanged = bool(
                base_section_sha is not None
                and head_section_sha is not None
                and base_section_sha == head_section_sha
            )
            if source_blob is None or head_document_blob is None:
                status = "unresolved"
            elif prose_unchanged:
                status = "needs_human_rationale"
            else:
                status = "prose_changed"
            obligations.append(
                {
                    "source_path": source_path,
                    "source_revision": head,
                    "source_sha256": hashlib.sha256(source_blob).hexdigest() if source_blob is not None else None,
                    "section_id": section_id,
                    "document_revision": head,
                    "document_sha256": hashlib.sha256(head_document_blob).hexdigest() if head_document_blob is not None else None,
                    "document_section_sha256": head_section_sha,
                    "base_document_revision": merge_base,
                    "base_document_sha256": hashlib.sha256(base_document_blob).hexdigest() if base_document_blob is not None else None,
                    "base_document_section_sha256": base_section_sha,
                    "prose_unchanged": prose_unchanged,
                    "status": status,
                }
            )
    return sorted(obligations, key=lambda item: (item["source_path"], item["section_id"]))


def _unique_object(pairs: list[tuple[str, Any]]) -> dict[str, Any]:
    result = {}
    for key, value in pairs:
        if key in result:
            raise AcknowledgmentError(f"duplicate JSON field {key!r}")
        result[key] = value
    return result


def evaluate(
    root: str,
    analysis: dict[str, Any],
    path: str | None,
) -> dict[str, Any]:
    """Validate supplied rationales against commit objects and attach them."""
    obligations = source_section_obligations(root, analysis)
    if path is None:
        return {"sha256": None, "source": None, "obligations": obligations, "entries": []}
    try:
        with open(path, "rb") as source:
            raw = source.read()
        payload = json.loads(raw.decode("utf-8"), object_pairs_hook=_unique_object)
    except (OSError, UnicodeDecodeError, json.JSONDecodeError) as error:
        raise AcknowledgmentError(f"cannot read acknowledgment JSON: {error}") from error
    if not isinstance(payload, dict) or set(payload) != {"schema_version", "acknowledgments"}:
        raise AcknowledgmentError("acknowledgment JSON must contain only schema_version and acknowledgments")
    if type(payload["schema_version"]) is not int or payload["schema_version"] != ACKNOWLEDGMENT_SCHEMA_VERSION:
        raise AcknowledgmentError(f"unsupported acknowledgment schema version {payload['schema_version']!r}")
    entries = payload["acknowledgments"]
    if not isinstance(entries, list):
        raise AcknowledgmentError("acknowledgments must be an array")

    by_key = {(item["source_path"], item["section_id"]): item for item in obligations}
    accepted = []
    seen: set[tuple[str, str]] = set()
    for index, entry in enumerate(entries, 1):
        if not isinstance(entry, dict) or set(entry) != ACKNOWLEDGMENT_FIELDS:
            raise AcknowledgmentError(f"acknowledgment #{index} has missing or unknown fields")
        if any(not isinstance(value, str) or not value.strip() for value in entry.values()):
            raise AcknowledgmentError(f"acknowledgment #{index} fields must be non-empty strings")
        key = (entry["source_path"], entry["section_id"])
        if key in seen:
            raise AcknowledgmentError(f"duplicate acknowledgment for {key[0]} -> {key[1]}")
        seen.add(key)
        obligation = by_key.get(key)
        if obligation is None:
            raise AcknowledgmentError(f"unresolved source/section acknowledgment {key[0]} -> {key[1]}")
        if obligation["status"] != "needs_human_rationale":
            raise AcknowledgmentError(f"acknowledgment is not valid for changed or unresolved prose: {key[1]}")
        identity_fields = (
            "source_revision",
            "source_sha256",
            "document_revision",
            "document_sha256",
            "document_section_sha256",
            "base_document_revision",
            "base_document_sha256",
            "base_document_section_sha256",
        )
        if any(entry[field] != obligation[field] for field in identity_fields):
            raise AcknowledgmentError(f"stale source/document identity in acknowledgment for {key[1]}")
        rationale = " ".join(entry["rationale"].split())
        accepted.append({**entry, "rationale": rationale, "status": "acknowledged"})
        obligation["status"] = "acknowledged"
        obligation["rationale"] = rationale
    return {
        "sha256": hashlib.sha256(raw).hexdigest(),
        "source": "caller-supplied-file",
        "obligations": obligations,
        "entries": accepted,
    }
