#!/usr/bin/env python3
"""
docs.py — Documentation inventory and link correctness checker.

Usage:
  python3 scripts/docs.py links --check [--root ROOT] [--contracts CONTRACTS] [--mode local|ci]
  python3 scripts/docs.py inventory [--root ROOT] [--contracts CONTRACTS] [--mode local|ci]
  python3 scripts/docs.py sync --check|--write [--root ROOT]
"""

import argparse
import os
import re
# Git is required to resolve same-repository links offline.
import subprocess  # nosec B404
import sys
import urllib.parse

from doc_tools import cli_sync, examples
from doc_tools.inventory import (
    discover_documents,
)
from doc_tools.markdown import MarkdownDocument

# Network and non-filesystem targets have no working-tree path to check; the
# tuple mirrors the skip contract recorded in docs/documentation-contract.md
# (plus protocol-relative URLs, which start like site paths).
NETWORK_URL_PREFIXES = ("http://", "https://", "mailto:", "tel:", "ftp://", "data:", "//")


def get_repo_owner_name(root_dir):
    """
    Attempts to extract github owner/repo string from git remote URL.
    """
    try:
        # Fixed argv (git named literally, no shell, output captured): the only
        # variable is a checkout path, never document- or network-controlled
        # input. The literal executable head keeps the command a static string
        # for static analysis, same shape as scripts/check_source_distribution.py.
        res = subprocess.run(  # nosec B603 B607
            ["git", "-C", root_dir, "remote", "get-url", "origin"],
            capture_output=True,
            text=True,
            check=True,
        )
        url = res.stdout.strip()
        # https://github.com/owner/repo or git@github.com:owner/repo
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
    # Pattern: https://github.com/<owner>/<repo>/(blob|raw)/<ref>/<path>[#frag]
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
            # A known origin pointing at another repository: a same-named local
            # file is a coincidence, not this link's target.
            return None, None

        # Origin unknown (e.g. fixture sandboxes): fall back to local existence.
        if os.path.exists(os.path.join(root_dir, path_part)):
            return path_part, frag

        return None, None

    # Relative GitHub link: /blob/<ref>/<path>
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

        # 1. Check all maintained documents
        for doc_rec in maintained_docs:
            doc = self.get_document(doc_rec.rel_path)
            if not doc:
                continue

            for link in doc.links:
                self._verify_link(doc, link)

        # 2. Check canonical alias documents and distribution context
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

            # Verify target document links in canonical context
            target_doc = self.get_document(target_rel)
            if target_doc:
                alias_dir = os.path.dirname(alias_rel)
                for link in target_doc.links:
                    raw_url = link.raw_url
                    # Absolute and site-absolute URLs resolve to the same
                    # location from any directory, so only relative links can
                    # change meaning in the alias's distribution context.
                    if raw_url.startswith(NETWORK_URL_PREFIXES) or raw_url.startswith("/"):
                        continue

                    url_path, _, _ = raw_url.partition("#")
                    url_path = url_path.split("?", 1)[0]
                    url_path = urllib.parse.unquote(url_path)
                    if url_path:
                        # Check distribution context: relative path from alias directory
                        dist_target = os.path.normpath(os.path.join(self.root_dir, alias_dir, url_path))
                        if not os.path.exists(dist_target):
                            self.warnings.append(
                                f"{alias_rel}:{link.line_no}: distribution-context notice: link '{url_path}' in canonical alias resolves outside alias directory"
                            )

        return len(self.errors) == 0

    def _verify_link(self, doc, link):
        raw_url = link.raw_url

        # Same-repo GitHub links become working-tree paths; other network-like
        # and site-absolute targets are skipped, as docs/documentation-contract.md
        # records. `/blob/<ref>/<path>` is GitHub's site-absolute form of a
        # repository file link and is still checked.
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

        # Same-document fragment link (#heading-anchor). `#top` is the one
        # fragment GitHub always provides regardless of heading content.
        if not url_path:
            if frag and frag != "top" and frag not in doc.anchors:
                self.errors.append(f"{doc.filepath}:{link.line_no}: missing anchor '#{frag}' in {doc.filepath}")
            return

        # Target file relative to current document's directory
        doc_dir = os.path.dirname(doc.filepath)
        target_rel = os.path.normpath(os.path.join(doc_dir, url_path)).replace("\\", "/")

        # Containment: `..` segments (and symlinks) must not leave the checkout.
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

        # The target must exist inside the checkout. GitHub serves
        # `/blob/<dir>` via a redirect to `/tree/<dir>`, and existence is the
        # documented target test (docs/documentation-contract.md), so existing
        # directories are valid link targets too.
        if not os.path.exists(target_real):
            self.errors.append(
                f"{doc.filepath}:{link.line_no}: broken link target '{url_path}' (file not found: {target_rel})"
            )
            return

        # If target has a fragment and target is a markdown file, check anchor existence
        if frag and frag != "top":
            if target_rel.lower().endswith(".md"):
                target_doc = self.get_document(target_rel)
                if target_doc and frag not in target_doc.anchors:
                    self.errors.append(
                        f"{doc.filepath}:{link.line_no}: missing anchor '#{frag}' in {target_rel}"
                    )


def main():
    parser = argparse.ArgumentParser(description="Documentation inventory and link correctness tool")
    subparsers = parser.add_subparsers(dest="subcommand", required=True)

    links_parser = subparsers.add_parser("links", help="Check document link correctness")
    links_parser.add_argument("--check", action="store_true", help="Perform offline link and anchor verification")
    links_parser.add_argument("--root", default=".", help="Root directory of repository")
    links_parser.add_argument("--contracts", default="docs/contracts.toml", help="Path to contracts policy file")
    links_parser.add_argument("--mode", choices=["local", "ci"], default="local", help="Discovery mode")

    inv_parser = subparsers.add_parser("inventory", help="Inspect documentation inventory")
    inv_parser.add_argument("--root", default=".", help="Root directory of repository")
    inv_parser.add_argument("--contracts", default="docs/contracts.toml", help="Path to contracts policy file")
    inv_parser.add_argument("--mode", choices=["local", "ci"], default="local", help="Discovery mode")

    examples_parser = subparsers.add_parser("examples", help="Verify documented CLI examples against real execution/fixtures")
    examples_parser.add_argument("--check", action="store_true", help="Perform example execution and verification")
    examples_parser.add_argument("--root", default=".", help="Root directory of repository")
    examples_parser.add_argument("--contracts", default="docs/contracts.toml", help="Path to contracts policy file")

    sync_parser = subparsers.add_parser("sync", help="Check or regenerate CLI-derived documentation blocks")
    sync_parser.add_argument("--check", action="store_true", help="Verify committed blocks against the CLI definitions (default)")
    sync_parser.add_argument("--write", action="store_true", help="Rewrite out-of-date blocks in place (refused while CI is set)")
    sync_parser.add_argument("--root", default=".", help="Root directory of repository")

    args = parser.parse_args()

    root_dir = os.path.abspath(args.root)

    if args.subcommand == "sync":
        if args.check and args.write:
            print("sync: --check and --write are mutually exclusive", file=sys.stderr)
            sys.exit(2)
        sys.exit(cli_sync.run_sync(root_dir, write=args.write))

    if args.subcommand == "examples":
        ok = examples.verify_examples(root_dir=root_dir)
        sys.exit(0 if ok else 1)

    contracts_path = os.path.join(root_dir, args.contracts) if not os.path.isabs(args.contracts) else args.contracts

    if args.subcommand == "inventory":
        try:
            res = discover_documents(root_dir, contracts_path, mode=args.mode)
            print(f"Discovered {len(res['records'])} total document(s) in contracts policy:")
            print(f"  - Maintained: {len(res['maintained'])}")
            print(f"  - Canonical Aliases: {len(res['canonical_aliases'])}")
            print(f"  - Historical: {len(res['historical'])}")
            print(f"  - Vendored: {len(res['vendored'])}")
            print(f"  - Fixtures: {len(res['fixture'])}")
            sys.exit(0)
        except Exception as e:
            print(f"Inventory failure: {e}", file=sys.stderr)
            sys.exit(1)

    if args.subcommand == "links":
        checker = LinkChecker(root_dir=root_dir, contracts_path=contracts_path, mode=args.mode)
        checker.check_links()

        if checker.warnings:
            for w in checker.warnings:
                print(w)

        if checker.errors:
            for e in checker.errors:
                print(e, file=sys.stderr)
            print(f"\nLink check FAILED: {len(checker.errors)} error(s) found.", file=sys.stderr)
            sys.exit(1)

        print("Link check OK: all local links and anchors are valid.")
        sys.exit(0)


if __name__ == "__main__":
    main()
