"""
inventory.py — Documentation discovery and contract classification matching.
"""

import os
import re
# Git is required to enumerate tracked markdown files offline.
import subprocess  # nosec B404
import sys

# Support Python 3.11+ built-in tomllib, fallback to tomli if needed
if sys.version_info >= (3, 11):
    import tomllib
else:
    try:
        import tomli as tomllib
    except ImportError:
        raise ImportError("tomllib (Python 3.11+) or tomli package is required to parse TOML files.")

ALLOWED_CLASSIFICATIONS = {
    "maintained",
    "historical",
    "fixture",
    "vendored",
    "canonical_alias",
    "canonical alias",
}


class PolicyError(Exception):
    """Raised when docs/contracts.toml is malformed or invalid."""
    pass


class PolicyContradictionError(PolicyError):
    """Raised when contract rules produce conflicting classifications at the same specificity."""
    pass


class UnclassifiedDocumentError(Exception):
    """Raised when a discovered document does not match any rule in contracts.toml."""
    pass


class Rule:
    def __init__(self, raw_dict, index):
        self.raw = raw_dict
        self.index = index
        self.path = raw_dict.get("path")
        self.glob = raw_dict.get("glob")
        if not self.path and not self.glob:
            raise PolicyError(f"Rule #{index} must specify 'path' or 'glob'")

        classification = raw_dict.get("classification")
        if not classification or classification not in ALLOWED_CLASSIFICATIONS:
            raise PolicyError(
                f"Rule #{index} has invalid classification '{classification}'. "
                f"Allowed: {sorted(list(ALLOWED_CLASSIFICATIONS))}"
            )
        # Normalize classification string
        self.classification = "canonical_alias" if classification in ("canonical_alias", "canonical alias") else classification

        self.reason = raw_dict.get("reason", "").strip()
        if not self.reason:
            raise PolicyError(f"Rule #{index} for '{self.path or self.glob}' must specify a non-empty 'reason'")

        self.canonical_target = raw_dict.get("canonical_target")
        if self.classification == "canonical_alias" and not self.canonical_target:
            raise PolicyError(f"Canonical alias rule #{index} ('{self.path or self.glob}') must specify 'canonical_target'")

        # Compile matching regex and calculate specificity
        if self.path:
            norm_p = os.path.normpath(self.path).replace("\\", "/")
            self.regex = re.compile(f"^{re.escape(norm_p)}$", re.IGNORECASE)
            self.specificity = 10000 + len(norm_p)
        else:
            self.regex, self.specificity = self._compile_glob(self.glob)

    def _compile_glob(self, glob_pat):
        norm_pat = os.path.normpath(glob_pat).replace("\\", "/")
        # Build regex
        fixed_prefix = norm_pat.split("*")[0].split("?")[0]
        spec = 1000 + len(fixed_prefix)

        # Convert glob to regex
        parts = norm_pat.split("**")
        regex_parts = []
        for part in parts:
            sub_parts = part.split("*")
            escaped_sub = [re.escape(s).replace(r"\?", "[^/]") for s in sub_parts]
            regex_parts.append("[^/]*".join(escaped_sub))

        full_regex = ".*".join(regex_parts)
        compiled = re.compile(f"^{full_regex}$", re.IGNORECASE)
        return compiled, spec

    def matches(self, rel_path):
        norm_path = os.path.normpath(rel_path).replace("\\", "/")
        return bool(self.regex.match(norm_path))


class ContractPolicy:
    def __init__(self, contracts_path):
        self.contracts_path = contracts_path
        if not os.path.exists(contracts_path):
            raise PolicyError(f"Contracts policy file not found: {contracts_path}")
        try:
            with open(contracts_path, "rb") as f:
                data = tomllib.load(f)
        except Exception as e:
            raise PolicyError(f"Failed to parse policy TOML at {contracts_path}: {e}")

        version = data.get("version")
        if version != 1:
            raise PolicyError(f"Unsupported contracts policy version {version} in {contracts_path} (expected version = 1)")

        rules_data = data.get("rules", [])
        if not isinstance(rules_data, list) or not rules_data:
            raise PolicyError(f"No rules defined in contracts policy at {contracts_path}")

        self.rules = [Rule(r, i + 1) for i, r in enumerate(rules_data)]

    def classify(self, rel_path):
        norm_path = os.path.normpath(rel_path).replace("\\", "/")
        matches = []
        for rule in self.rules:
            if rule.matches(norm_path):
                matches.append(rule)

        if not matches:
            return None

        # Sort by specificity descending
        matches.sort(key=lambda r: r.specificity, reverse=True)
        top_spec = matches[0].specificity
        top_matches = [m for m in matches if m.specificity == top_spec]

        # Check for contradictions among top matches
        first_cls = top_matches[0].classification
        first_target = top_matches[0].canonical_target
        for m in top_matches[1:]:
            if m.classification != first_cls or m.canonical_target != first_target:
                raise PolicyContradictionError(
                    f"Contradictory rules match '{rel_path}' at specificity {top_spec}: "
                    f"'{m.classification}' vs '{first_cls}'"
                )

        return top_matches[0]


class DocumentRecord:
    def __init__(self, rel_path, classification, reason, rule=None, canonical_target=None):
        self.rel_path = os.path.normpath(rel_path).replace("\\", "/")
        self.classification = classification
        self.reason = reason
        self.rule = rule
        self.canonical_target = os.path.normpath(canonical_target).replace("\\", "/") if canonical_target else None

    def __repr__(self):
        return f"<DocumentRecord {self.rel_path} [{self.classification}]>"


def _ls_files(root_dir, flags, pathspec=None):
    """
    Runs `git ls-files` in a checkout and returns its non-empty output lines.
    """
    # Fixed argv (git named literally, no shell, output captured): the only
    # variables are a checkout path plus caller-fixed flags and pathspecs, never
    # document- or network-controlled input. The literal executable head keeps
    # the command a static string for static analysis, same shape as
    # scripts/check_source_distribution.py.
    optional_pathspec = [] if pathspec is None else [pathspec]
    res = subprocess.run(  # nosec B603 B607
        ["git", "-C", root_dir, "ls-files", *flags, *optional_pathspec],
        capture_output=True,
        text=True,
        check=True,
    )
    return [line.strip() for line in res.stdout.splitlines() if line.strip()]


def _get_git_files(root_dir, mode="local"):
    """
    Returns list of relative paths for markdown files using git.

    "local" mode discovers tracked files plus files that are untracked but not
    ignored, so a maintained document is checked before it is staged; other
    modes see the tracked tree only. Git's `:(icase)` pathspec also matches the
    README.MD-style casing the default case-sensitive glob misses, but in
    `--others` mode it silently returns nothing when the checkout root is a
    nested directory absent from the index (verified on git 2.53), so the
    untracked half is enumerated without a pathspec and matched case-
    insensitively here instead.
    """
    try:
        # Tracked half: the case-insensitive pathspec is reliable for the index.
        files = _ls_files(root_dir, ["--cached"] if mode == "local" else [], ":(icase)*.md")
        if mode == "local":
            # `--exclude-standard` keeps .gitignore semantics identical; the
            # suffix match below reproduces the pathspec's case-insensitivity.
            files.extend(
                f
                for f in _ls_files(root_dir, ["--others", "--exclude-standard"])
                if f.lower().endswith(".md")
            )
        return files
    except (subprocess.CalledProcessError, FileNotFoundError, OSError):
        # Fallback to filesystem walk if git fails or is unavailable
        md_files = []
        for dirpath, _, filenames in os.walk(root_dir):
            for f in filenames:
                if f.lower().endswith(".md"):
                    full = os.path.join(dirpath, f)
                    rel = os.path.relpath(full, root_dir)
                    md_files.append(rel.replace("\\", "/"))
        return md_files


def discover_documents(root_dir, contracts_path, mode="local"):
    """
    Discovers all Markdown documentation files and classifies them against contracts.toml.
    Returns dict containing classified document records.
    """
    root_dir = os.path.abspath(root_dir)
    policy = ContractPolicy(contracts_path)

    all_md_files = _get_git_files(root_dir, mode=mode)

    # Filter to document discovery rules:
    # 1. Case-insensitive README.md anywhere in tree
    # 2. Any Markdown file under docs/
    # 3. Explicitly configured maintained documents or matched rule targets
    discovered_files = set()

    for rel in all_md_files:
        norm_rel = os.path.normpath(rel).replace("\\", "/")
        filename = os.path.basename(norm_rel).lower()

        # Check discovery criteria
        is_readme = filename == "readme.md" or filename.startswith("readme")
        is_under_docs = norm_rel.startswith("docs/") or norm_rel.startswith("docs\\")

        # Check rule match
        rule = policy.classify(norm_rel)

        if is_readme or is_under_docs or (rule and rule.classification in ("maintained", "canonical_alias")):
            discovered_files.add(norm_rel)

    if not discovered_files:
        raise PolicyError("No documentation files were discovered.")

    classified_records = {}
    unclassified = []

    for rel in sorted(discovered_files):
        rule = policy.classify(rel)
        if not rule:
            unclassified.append(rel)
        else:
            classified_records[rel] = DocumentRecord(
                rel_path=rel,
                classification=rule.classification,
                reason=rule.reason,
                rule=rule,
                canonical_target=rule.canonical_target,
            )

    if unclassified:
        raise UnclassifiedDocumentError(
            f"Unclassified document(s) discovered without a matching rule in {contracts_path}:\n"
            + "\n".join(f"  - {u}" for u in unclassified)
            + f"\nPlease add an explicit classification rule with a reason to {contracts_path}."
        )

    maintained = [r for r in classified_records.values() if r.classification == "maintained"]
    historical = [r for r in classified_records.values() if r.classification == "historical"]
    fixture = [r for r in classified_records.values() if r.classification == "fixture"]
    vendored = [r for r in classified_records.values() if r.classification == "vendored"]
    canonical_aliases = [r for r in classified_records.values() if r.classification == "canonical_alias"]

    if not maintained and not canonical_aliases:
        raise PolicyError(f"Maintained documentation inventory in {contracts_path} is empty.")

    return {
        "policy": policy,
        "records": classified_records,
        "maintained": maintained,
        "historical": historical,
        "fixture": fixture,
        "vendored": vendored,
        "canonical_aliases": canonical_aliases,
    }
