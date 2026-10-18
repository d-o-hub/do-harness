"""
links.py — Document link and anchor correctness checking.
"""

import os
import re
import subprocess  # nosec B404
import urllib.parse

from doc_tools.inventory import discover_documents
from doc_tools.markdown import MarkdownDocument

NETWORK_URL_PREFIXES = ("http://", "https://", "mailto:", "tel:", "ftp://", "data:", "//")


def get_repo_owner_name(root_dir):
    """Attempts to extract github owner/repo string from git remote URL."""
    try:
        res = subprocess.run(  # nosec B603 B607
            ["git", "-C", root_dir, "remote", "get-url", "origin"],
            capture_output=True,
            text=True,
            check=True,
        )
        url = res.stdout.strip()
        m = re.search(r"github\.com[:/]([^/]+)/([^/.]+)(?:\.git)?$", url, re.IGNORECASE)
        if m:
            return f"{m.group(1)}/{m.group(2)}"
    except (subprocess.CalledProcessError, FileNotFoundError, OSError):
        pass
    return None


def normalize_github_link(url, root_dir, repo_owner_name=None):
    """
    Normalizes same-repository GitHub blob/raw links to local relative file paths and fragments.
    Returns (local_rel_path, fragment) or (None, None) if not a same-repo GitHub link.
    """
    m = re.match(
        r"^https?://github\.com/([^/]+)/([^/]+)/(?:blob|raw)/[^/]+/(.+)$",
        url,
        re.IGNORECASE,
    )
    if m:
        owner, repo, path_and_frag = m.group(1), m.group(2), m.group(3)
        path_part, _, frag = path_and_frag.partition("#")
        path_part = urllib.parse.unquote(path_part)

        if repo_owner_name:
            curr_owner, curr_repo = repo_owner_name.split("/")
            if owner.lower() == curr_owner.lower() and repo.lower().replace(".git", "") == curr_repo.lower():
                return path_part, frag
            return None, None

        if os.path.exists(os.path.join(root_dir, path_part)):
            return path_part, frag

        return None, None

    m2 = re.match(r"^/blob/[^/]+/(.+)$", url, re.IGNORECASE)
    if m2:
        path_and_frag = m2.group(1)
        path_part, _, frag = path_and_frag.partition("#")
        path_part = urllib.parse.unquote(path_part)
        return path_part, frag

    return None, None


class LinkChecker:
    def __init__(self, root_dir, contracts_path="docs/contracts.toml", mode="local"):
        self.root_dir = os.path.abspath(root_dir)
        self.contracts_path = os.path.join(self.root_dir, contracts_path) if not os.path.isabs(contracts_path) else contracts_path
        self.mode = mode
        self.repo_owner_name = get_repo_owner_name(self.root_dir)
        self.doc_cache = {}
        self.errors = []
        self.warnings = []

    def get_document(self, rel_path):
        norm_rel = os.path.normpath(rel_path).replace("\\", "/")
        if norm_rel in self.doc_cache:
            return self.doc_cache[norm_rel]

        full_path = os.path.join(self.root_dir, norm_rel)
        if not os.path.isfile(full_path):
            return None

        try:
            with open(full_path, "r", encoding="utf-8") as f:
                content = f.read()
            doc = MarkdownDocument(norm_rel, content)
            self.doc_cache[norm_rel] = doc
            return doc
        except Exception as e:
            self.errors.append(f"{norm_rel}:1: Failed to read or parse markdown file: {e}")
            return None

    def check_links(self):
        try:
            inventory = discover_documents(self.root_dir, self.contracts_path, mode=self.mode)
        except Exception as e:
            self.errors.append(f"Inventory discovery failed: {e}")
            return False

        maintained_docs = inventory["maintained"]
        canonical_aliases = inventory["canonical_aliases"]

        for doc_rec in maintained_docs:
            doc = self.get_document(doc_rec.rel_path)
            if not doc:
                continue

            for link in doc.links:
                self._verify_link(doc, link)

        for alias_rec in canonical_aliases:
            alias_rel = alias_rec.rel_path
            target_rel = alias_rec.canonical_target

            alias_full = os.path.join(self.root_dir, alias_rel)
            target_full = os.path.join(self.root_dir, target_rel)

            if not os.path.exists(alias_full):
                self.errors.append(f"{alias_rel}:1: Canonical alias file does not exist")
                continue

            if not os.path.exists(target_full):
                self.errors.append(f"{alias_rel}:1: Canonical target '{target_rel}' does not exist")
                continue

            target_doc = self.get_document(target_rel)
            if target_doc:
                alias_dir = os.path.dirname(alias_rel)
                for link in target_doc.links:
                    raw_url = link.raw_url
                    if raw_url.startswith(NETWORK_URL_PREFIXES) or raw_url.startswith("/"):
                        continue

                    url_path, _, _ = raw_url.partition("#")
                    url_path = url_path.split("?", 1)[0]
                    url_path = urllib.parse.unquote(url_path)
                    if url_path:
                        dist_target = os.path.normpath(os.path.join(self.root_dir, alias_dir, url_path))
                        if not os.path.exists(dist_target):
                            self.warnings.append(
                                f"{alias_rel}:{link.line_no}: distribution-context notice: link '{url_path}' in canonical alias resolves outside alias directory"
                            )

        return len(self.errors) == 0

    def _verify_link(self, doc, link):
        raw_url = link.raw_url

        if raw_url.startswith(NETWORK_URL_PREFIXES) or raw_url.startswith("/"):
            gh_path, gh_frag = normalize_github_link(raw_url, self.root_dir, self.repo_owner_name)
            if gh_path:
                norm_url = gh_path + (f"#{gh_frag}" if gh_frag else "")
            else:
                return
        else:
            norm_url = raw_url

        url_path, _, frag = norm_url.partition("#")
        url_path = url_path.split("?", 1)[0]
        url_path = urllib.parse.unquote(url_path)
        frag = urllib.parse.unquote(frag)

        if not url_path:
            if frag and frag != "top" and frag not in doc.anchors:
                self.errors.append(f"{doc.filepath}:{link.line_no}: missing anchor '#{frag}' in {doc.filepath}")
            return

        doc_dir = os.path.dirname(doc.filepath)
        target_rel = os.path.normpath(os.path.join(doc_dir, url_path)).replace("\\", "/")

        root_real = os.path.realpath(self.root_dir)
        target_real = os.path.realpath(os.path.join(self.root_dir, target_rel))
        try:
            inside_root = os.path.commonpath([root_real, target_real]) == root_real
        except ValueError:
            inside_root = False

        if not inside_root:
            self.errors.append(
                f"{doc.filepath}:{link.line_no}: link target '{url_path}' escapes the repository root"
            )
            return

        if not os.path.exists(target_real):
            self.errors.append(
                f"{doc.filepath}:{link.line_no}: broken link target '{url_path}' (file not found: {target_rel})"
            )
            return

        if frag and frag != "top":
            if target_rel.lower().endswith(".md"):
                target_doc = self.get_document(target_rel)
                if target_doc and frag not in target_doc.anchors:
                    self.errors.append(
                        f"{doc.filepath}:{link.line_no}: missing anchor '#{frag}' in {target_rel}"
                    )
