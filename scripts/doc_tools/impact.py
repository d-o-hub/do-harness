"""
impact.py — Deterministic Git diff analysis and doc section impact mapping.
"""

import os
import subprocess  # nosec B404

from doc_tools.inventory import ContractPolicy, PolicyError


def resolve_commit(root_dir, rev):
    """Resolves a git revision to full commit hash."""
    try:
        res = subprocess.run(  # nosec B603 B607
            ["git", "-C", root_dir, "rev-parse", "--verify", f"{rev}^{{commit}}"],
            capture_output=True,
            text=True,
            check=True,
        )
        return res.stdout.strip()
    except (subprocess.CalledProcessError, FileNotFoundError, OSError) as e:
        raise ValueError(f"Unresolvable revision '{rev}': {e}")


def get_merge_base(root_dir, base_rev, head_rev):
    """Calculates merge base commit between base_rev and head_rev."""
    base_commit = resolve_commit(root_dir, base_rev)
    head_commit = resolve_commit(root_dir, head_rev)
    try:
        res = subprocess.run(  # nosec B603 B607
            ["git", "-C", root_dir, "merge-base", base_commit, head_commit],
            capture_output=True,
            text=True,
            check=True,
        )
        return res.stdout.strip()
    except (subprocess.CalledProcessError, FileNotFoundError, OSError) as e:
        raise ValueError(f"Failed to find merge base between '{base_rev}' and '{head_rev}': {e}")


def load_file_bytes_at_commit(root_dir, commit, rel_path):
    """Loads file bytes at a specific git commit or returns None if not found."""
    try:
        res = subprocess.run(  # nosec B603 B607
            ["git", "-C", root_dir, "show", f"{commit}:{rel_path}"],
            capture_output=True,
            check=True,
        )
        return res.stdout
    except (subprocess.CalledProcessError, FileNotFoundError, OSError):
        return None


def parse_git_diff_name_status(root_dir, base_commit, head_commit):
    """
    Runs `git diff -z --name-status base head` and parses NUL-delimited output.
    Returns list of dicts: {'status': str, 'path': str, 'old_path': str|None}
    Status codes: A, M, D, R100, C100, etc.
    """
    try:
        res = subprocess.run(  # nosec B603 B607
            ["git", "-C", root_dir, "diff", "-z", "--name-status", base_commit, head_commit],
            capture_output=True,
            check=False,
        )
        if res.returncode != 0:
            raise ValueError(f"git diff failed (exit {res.returncode}): {res.stderr.decode('utf-8', errors='replace')}")
        raw_bytes = res.stdout
    except (FileNotFoundError, OSError) as e:
        raise ValueError(f"Failed to execute git diff: {e}")

    items = raw_bytes.split(b"\x00")
    changes = []
    i = 0
    n = len(items)

    while i < n:
        item = items[i]
        if not item:
            i += 1
            continue

        status_str = item.decode("utf-8", errors="replace").strip()
        status_code = status_str[0] if status_str else ""

        if status_code in ("R", "C"):
            # Renames and copies have status, old_path, new_path
            if i + 2 < n:
                old_path = items[i + 1].decode("utf-8", errors="replace")
                new_path = items[i + 2].decode("utf-8", errors="replace")
                changes.append({
                    "status": status_code,
                    "status_full": status_str,
                    "path": os.path.normpath(new_path).replace("\\", "/"),
                    "old_path": os.path.normpath(old_path).replace("\\", "/"),
                })
                i += 3
                continue
            else:
                break
        else:
            # Added, Modified, Deleted, etc.
            if i + 1 < n:
                path = items[i + 1].decode("utf-8", errors="replace")
                changes.append({
                    "status": status_code,
                    "status_full": status_str,
                    "path": os.path.normpath(path).replace("\\", "/"),
                    "old_path": None,
                })
                i += 2
                continue
            else:
                break

    return changes


def get_diff_hunks_for_file(root_dir, base_commit, head_commit, rel_path):
    """Gets patch diff text for a given file relative path between base and head."""
    try:
        res = subprocess.run(  # nosec B603 B607
            ["git", "-C", root_dir, "diff", base_commit, head_commit, "--", rel_path],
            capture_output=True,
            text=True,
            check=False,
        )
        if res.returncode == 0:
            return res.stdout
    except (FileNotFoundError, OSError):
        pass
    return ""


POLICY_AND_CHECKER_PATHS = {
    "docs/contracts.toml",
    "scripts/docs.py",
    "scripts/doc_tools/inventory.py",
    "scripts/doc_tools/markdown.py",
    "scripts/doc_tools/impact.py",
    "scripts/doc_tools/report.py",
    "scripts/doc_tools/examples.py",
    "scripts/doc_tools/cli_sync.py",
    "scripts/test_doc_tools.py",
    "tests/fixtures/docs-examples/manifest.toml",
    "tests/fixtures/cli-contract.json",
}


def analyze_impact(root_dir, base_rev, head_rev, contracts_path="docs/contracts.toml"):
    """
    Performs deterministic docs impact analysis between base_rev and head_rev.
    Loads ContractPolicy from merge-base commit.
    Returns impact structure dictionary.
    """
    root_dir = os.path.abspath(root_dir)
    merge_base = get_merge_base(root_dir, base_rev, head_rev)
    head_commit = resolve_commit(root_dir, head_rev)

    # 1. Load ContractPolicy from merge base
    policy_bytes = load_file_bytes_at_commit(root_dir, merge_base, contracts_path)
    if policy_bytes is None:
        raise PolicyError(f"Contracts policy file '{contracts_path}' not found at merge base commit {merge_base[:8]}")

    policy = ContractPolicy(contracts_path, content_bytes=policy_bytes)

    # 2. Get file changes
    changes = parse_git_diff_name_status(root_dir, merge_base, head_commit)

    policy_tampering = False
    checker_code_changed = False
    affected_doc_sections = {}  # doc_rel_path -> set of section_ids
    source_reasons = {}  # doc_rel_path -> list of reasons
    unknown_scope_files = []
    changed_doc_files = set()
    changed_source_files = set()

    for change in changes:
        path = change["path"]
        old_path = change["old_path"]
        endpoints = [path]
        if old_path:
            endpoints.append(old_path)

        # Check policy and checker tampering
        for ep in endpoints:
            if ep == contracts_path or ep.startswith("docs/contracts.toml"):
                policy_tampering = True
            if ep in POLICY_AND_CHECKER_PATHS or ep.startswith("scripts/doc_tools/"):
                checker_code_changed = True

        # Check if change is a doc file or source file
        for ep in endpoints:
            rule = policy.classify(ep)
            if rule and rule.classification in ("maintained", "canonical_alias"):
                changed_doc_files.add(ep)
            else:
                changed_source_files.add(ep)

            # Map source file through source_mappings
            mapped = False
            for sm in policy.source_mappings:
                if sm.matches(ep):
                    mapped = True
                    for target_doc in sm.target_docs:
                        if target_doc not in affected_doc_sections:
                            affected_doc_sections[target_doc] = set()
                            source_reasons[target_doc] = []
                        affected_doc_sections[target_doc].update(sm.target_sections)
                        if sm.reason and sm.reason not in source_reasons[target_doc]:
                            source_reasons[target_doc].append(sm.reason)

            # If an unmapped source file changed (and not a classified doc file, fixture, historical, or policy file)
            is_doc = rule and rule.classification in ("maintained", "canonical_alias", "historical", "fixture", "vendored")
            if not mapped and not is_doc:
                if ep not in unknown_scope_files:
                    unknown_scope_files.append(ep)

    # Convert sets to sorted lists for deterministic output
    formatted_affected = {}
    for doc_path, sections in sorted(affected_doc_sections.items()):
        formatted_affected[doc_path] = {
            "sections": sorted(list(sections)),
            "reasons": sorted(source_reasons.get(doc_path, [])),
        }

    return {
        "merge_base": merge_base,
        "head_commit": head_commit,
        "changes": changes,
        "policy_tampering": policy_tampering,
        "checker_code_changed": checker_code_changed,
        "changed_doc_files": sorted(list(changed_doc_files)),
        "changed_source_files": sorted(list(changed_source_files)),
        "affected_doc_sections": formatted_affected,
        "unknown_scope_files": sorted(unknown_scope_files),
        "policy": policy,
    }
