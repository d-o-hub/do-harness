"""Commit-range analysis for documentation impact and review packets.

Public contract: ``analyze(root, base_ref, head_ref)`` returns a deterministic
mapping or raises ``ImpactError``. The source map and document classifications
come only from the merge-base policy. Git is invoked with fixed argv and never
through a shell.
"""

from __future__ import annotations

import fnmatch
import hashlib
import os
import re
from dataclasses import asdict, dataclass
from typing import Any

try:
    import tomllib
except ImportError:  # Python 3.10 compatibility matches inventory.py.
    import tomli as tomllib

from doc_tools.git_helpers import (
    ImpactError,
    checker_digest as _checker_digest,
    checker_tree as _checker_tree,
    checkout_identity as _checkout_identity,
    git as _git,
    parse_name_status as _parse_name_status,
    read_blob as _read_blob,
    resolve_commit as _resolve_commit,
    tree_identity as _tree_identity,
)

from doc_tools.inventory import Rule
from doc_tools.sections import (
    Section,
    changed_line_sets,
    deepest_sections_for_lines,
    extract_sections,
)

POLICY_PATH = "docs/contracts.toml"
CHECKER_PATTERNS = (
    "Cargo.toml",
    "Cargo.lock",
    "crates/do-harness/Cargo.toml",
    "crates/do-harness/build.rs",
    ".cargo/**",
    "rust-toolchain.toml",
    "scripts/docs.py",
    "scripts/doc_tools/**",
    "scripts/test_doc_tools.py",
    "scripts/tests/**",
    "scripts/requirements-docs.txt",
    "crates/do-harness/src/docs_generation/**",
    "tests/fixtures/cli-contract.json",
    "tests/fixtures/docs-examples/**",
)
RELEVANT_SOURCE_PREFIXES = (
    "crates/do-harness/src/",
    "crates/do-harness/templates/",
    ".github/workflows/",
)
RELEVANT_SOURCE_FILES = {"do-harness.toml"}


@dataclass(frozen=True)
class ImpactRule:
    pattern: str
    exact: bool
    sections: tuple[str, ...]
    reason: str
    specificity: int

    def matches(self, path: str) -> bool:
        return path == self.pattern if self.exact else fnmatch.fnmatchcase(path, self.pattern)




def _parse_policy(raw: bytes) -> tuple[list[Rule], list[ImpactRule], str]:
    try:
        policy = tomllib.loads(raw.decode("utf-8"))
    except (UnicodeDecodeError, tomllib.TOMLDecodeError) as error:
        raise ImpactError(f"merge-base documentation policy is invalid: {error}") from error
    if policy.get("version") != 1:
        raise ImpactError("merge-base docs/contracts.toml must declare version = 1")
    raw_rules = policy.get("rules")
    if not isinstance(raw_rules, list) or not raw_rules:
        raise ImpactError("merge-base docs/contracts.toml has no document classification rules")
    try:
        document_rules = [Rule(value, index + 1) for index, value in enumerate(raw_rules)]
    except Exception as error:
        raise ImpactError(f"merge-base document classification policy is invalid: {error}") from error

    map_data = policy.get("impact_map", [])
    if not isinstance(map_data, list):
        raise ImpactError("merge-base impact_map must be an array of tables")
    impact_rules: list[ImpactRule] = []
    for index, value in enumerate(map_data, 1):
        if not isinstance(value, dict):
            raise ImpactError(f"impact_map entry #{index} is not a table")
        path, glob = value.get("path"), value.get("glob")
        if bool(path) == bool(glob):
            raise ImpactError(f"impact_map entry #{index} must specify exactly one of path or glob")
        pattern = path or glob
        sections = value.get("sections")
        reason = value.get("reason")
        if not isinstance(pattern, str) or not pattern:
            raise ImpactError(f"impact_map entry #{index} has an empty path/glob")
        if not isinstance(sections, list) or any(
            not isinstance(section, str) or "#" not in section or section.startswith("#")
            for section in sections
        ):
            raise ImpactError(f"impact_map entry #{index} has invalid sections")
        if not isinstance(reason, str) or not reason.strip():
            raise ImpactError(f"impact_map entry #{index} needs a non-empty reason")
        exact = path is not None
        specificity = (10_000 if exact else 1_000) + len(pattern.replace("*", "").replace("?", ""))
        impact_rules.append(ImpactRule(pattern, exact, tuple(sections), reason.strip(), specificity))
    return document_rules, impact_rules, hashlib.sha256(raw).hexdigest()


def _classify_document(path: str, rules: list[Rule]) -> str | None:
    matches = [rule for rule in rules if rule.matches(path)]
    if not matches:
        return None
    best = max(rule.specificity for rule in matches)
    winners = [rule for rule in matches if rule.specificity == best]
    identity = {(rule.classification, rule.canonical_target) for rule in winners}
    if len(identity) > 1:
        raise ImpactError(f"merge-base document policy contradicts itself for {path}")
    return winners[0].classification


def _selected_map(path: str, rules: list[ImpactRule]) -> ImpactRule | None:
    matches = [rule for rule in rules if rule.matches(path)]
    if not matches:
        return None
    best = max(rule.specificity for rule in matches)
    winners = [rule for rule in matches if rule.specificity == best]
    if len({(rule.sections, rule.reason) for rule in winners}) > 1:
        raise ImpactError(f"merge-base impact_map has contradictory rules for {path}")
    return winners[0]



def _section_for_id(root: str, base: str, head: str, section_id: str) -> tuple[Section | None, bool]:
    path = section_id.split("#", 1)[0]
    for revision, is_head in ((head, True), (base, False)):
        blob = _read_blob(root, revision, path)
        if blob is None:
            continue
        try:
            sections = extract_sections(path, blob.decode("utf-8"))
        except (UnicodeDecodeError, ValueError, RuntimeError):
            return None, False
        found = next((section for section in sections if section.section_id == section_id), None)
        if found is not None:
            return found, is_head
    return None, False


def _is_checker_path(path: str) -> bool:
    return any(fnmatch.fnmatchcase(path, pattern) for pattern in CHECKER_PATTERNS)


def _is_relevant_source(path: str) -> bool:
    return path in RELEVANT_SOURCE_FILES or path.startswith(RELEVANT_SOURCE_PREFIXES)




def analyze(root: str, base_ref: str, head_ref: str) -> dict[str, Any]:
    """Analyze a commit range using merge-base policy and NUL-safe Git paths."""
    root = os.path.abspath(root)
    base = _resolve_commit(root, base_ref, "base")
    head = _resolve_commit(root, head_ref, "head")
    merge_base = _git(root, "merge-base", base, head).decode("ascii", "strict").strip().lower()
    if not re.fullmatch(r"[0-9a-f]{40,64}", merge_base):
        raise ImpactError("git merge-base did not return a commit")
    changes = _parse_name_status(_git(root, "diff", "--name-status", "-z", "--find-renames", merge_base, head))
    policy_bytes = _read_blob(root, merge_base, POLICY_PATH)
    if policy_bytes is None:
        raise ImpactError(f"merge-base has no {POLICY_PATH}; refusing to analyze with head policy")
    document_rules, impact_rules, policy_sha = _parse_policy(policy_bytes)
    policy_changed = _read_blob(root, head, POLICY_PATH) != policy_bytes
    paths = sorted({path for change in changes for path in (change.old_path, change.new_path) if path is not None})

    unknown: list[dict[str, str]] = []
    source_reasons: list[dict[str, Any]] = []
    mapped_sections: dict[str, set[str]] = {}
    reviewable_paths: set[str] = set()
    for path in paths:
        if path.lower().endswith(".md"):
            classification = _classify_document(path, document_rules)
            if classification not in ("maintained", "canonical_alias"):
                unknown.append({"path": path, "reason": "changed Markdown is not maintained by the merge-base documentation policy"})
            elif any(
                change.status[0] in ("R", "D") and path in (change.old_path, change.new_path)
                for change in changes
            ):
                unknown.append({"path": path, "reason": "Markdown rename or deletion requires raw review"})
            continue
        if _is_checker_path(path) or path == POLICY_PATH:
            reviewable_paths.add(path)
        if _is_relevant_source(path):
            reviewable_paths.add(path)
            if any(
                change.status[0] in ("R", "D") and path in (change.old_path, change.new_path)
                for change in changes
            ):
                unknown.append({"path": path, "reason": "relevant source rename or deletion requires raw review"})
            rule = _selected_map(path, impact_rules)
            if rule is None:
                unknown.append({"path": path, "reason": "relevant source path has no merge-base impact_map rule"})
                source_reasons.append({"path": path, "reason": "unmapped relevant source path", "sections": [], "mapped": False})
            else:
                mapped = bool(rule.sections)
                source_reasons.append({"path": path, "reason": rule.reason, "sections": list(rule.sections), "mapped": mapped})
                for section_id in rule.sections:
                    mapped_sections.setdefault(section_id, set()).add(path)
                if not mapped:
                    unknown.append({"path": path, "reason": rule.reason})
        elif not _is_checker_path(path) and path != POLICY_PATH:
            reviewable_paths.add(path)
            unknown.append({"path": path, "reason": "changed non-Markdown input has no documentation impact classification"})

    affected: dict[str, Section] = {}

    def record_section(section: Section, source_path: str | None, prefer: bool) -> None:
        previous = affected.get(section.section_id)
        source_paths = set(section.source_paths)
        if previous is not None:
            source_paths.update(previous.source_paths)
        if source_path is not None:
            source_paths.add(source_path)
        section.source_paths = sorted(source_paths)
        if previous is None or prefer:
            affected[section.section_id] = section
    for change in changes:
        old_path, new_path = change.old_path, change.new_path
        if not any(path and path.lower().endswith(".md") for path in (old_path, new_path)):
            continue
        old_blob = _read_blob(root, merge_base, old_path)
        new_blob = _read_blob(root, head, new_path)
        try:
            old_text = old_blob.decode("utf-8") if old_blob is not None else ""
            new_text = new_blob.decode("utf-8") if new_blob is not None else ""
            old_sections = extract_sections(old_path, old_text) if old_path and old_blob is not None else []
            new_sections = extract_sections(new_path, new_text) if new_path and new_blob is not None else []
        except (UnicodeDecodeError, ValueError, RuntimeError) as error:
            unknown.append({"path": new_path or old_path or "(unknown)", "reason": f"unsupported Markdown input: {error}"})
            continue
        old_changed, new_changed = changed_line_sets(old_text, new_text)
        if not old_changed and not new_changed and change.status.startswith("R"):
            old_changed.update(range(1, len(old_text.splitlines()) + 1))
            new_changed.update(range(1, len(new_text.splitlines()) + 1))
        old_affected = deepest_sections_for_lines(old_sections, old_changed)
        old_affected_ids = {section.section_id for section in old_affected}
        new_affected = {
            section.section_id: section
            for section in deepest_sections_for_lines(new_sections, new_changed)
        }
        for section in old_affected:
            record_section(section, old_path or section.path, prefer=False)
        for section in new_sections:
            if section.section_id in old_affected_ids:
                new_affected[section.section_id] = section
        for section in new_affected.values():
            record_section(section, new_path or section.path, prefer=True)

    for section_id, source_paths in mapped_sections.items():
        section, from_head = _section_for_id(root, merge_base, head, section_id)
        if section is None:
            unknown.append({"path": section_id.split("#", 1)[0], "reason": f"mapped section {section_id} is missing or unsupported"})
            continue
        section.source_paths.extend(sorted(source_paths))
        record_section(section, None, prefer=from_head)
        if not from_head:
            unknown.append({"path": section.path, "reason": f"mapped section {section_id} was deleted or renamed in the head revision"})

    diff_paths = sorted(reviewable_paths)
    diff_raw = b""
    if diff_paths:
        diff_raw = _git(
            root, "diff", "--no-ext-diff", "--no-color", "--find-renames", "--unified=3",
            merge_base, head, "--", *diff_paths,
        )
    checker_base = _checker_tree(root, merge_base, CHECKER_PATTERNS)
    checker_head = _checker_tree(root, head, CHECKER_PATTERNS)
    checker_changed_paths = sorted(
        path for path in set(checker_base) | set(checker_head)
        if checker_base.get(path) != checker_head.get(path)
    )
    checker_base_sha, checker_head_sha = _checker_digest(checker_base), _checker_digest(checker_head)
    affected_document_paths = sorted(
        {path for change in changes for path in (change.old_path, change.new_path)
         if path is not None and path.lower().endswith(".md")}
        | {section.path for section in affected.values()}
    )
    return {
        "revisions": {"base": base, "head": head, "merge_base": merge_base},
        "changes": [asdict(change) for change in changes],
        "changed_paths": paths,
        "policy": {
            "path": POLICY_PATH,
            "source_revision": merge_base,
            "sha256": policy_sha,
            "changed_at_head": policy_changed,
            "impact_map_present_at_base": bool(impact_rules),
        },
        "checker": {
            "paths": list(CHECKER_PATTERNS),
            "changed_paths": checker_changed_paths,
            "base_sha256": checker_base_sha,
            "head_sha256": checker_head_sha,
            "unchanged": checker_base_sha == checker_head_sha,
        },
        "inputs": {
            "head_tree": _tree_identity(root, head),
            "diff_sha256": hashlib.sha256(diff_raw).hexdigest(),
        },
        "checkout": _checkout_identity(root, head),
        "source_path_reasons": sorted(source_reasons, key=lambda item: item["path"]),
        "affected_sections": [affected[key].as_dict() for key in sorted(affected)],
        "affected_document_paths": affected_document_paths,
        "unknown_scope": sorted(unknown, key=lambda item: (item["path"], item["reason"])),
        "reviewable_paths": diff_paths,
        "source_diff": diff_raw.decode("utf-8", "replace"),
    }


def full_document_inputs(root: str, analysis: dict[str, Any]) -> list[dict[str, str]]:
    """Return complete affected-document text for raw-input byte comparisons."""
    root = os.path.abspath(root)
    head = analysis["revisions"]["head"]
    merge_base = analysis["revisions"]["merge_base"]
    result = []
    for path in analysis.get("affected_document_paths", []):
        revision = head
        raw = _read_blob(root, head, path)
        if raw is None:
            revision = merge_base
            raw = _read_blob(root, merge_base, path)
        if raw is None:
            continue
        result.append(
            {
                "path": path,
                "revision": revision,
                "sha256": hashlib.sha256(raw).hexdigest(),
                "content": raw.decode("utf-8", "replace"),
            }
        )
    return result
