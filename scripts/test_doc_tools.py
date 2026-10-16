#!/usr/bin/env python3
"""
test_doc_tools.py — Unit and integration tests for inventory discovery, contract policy,
markdown parsing, and link verification.
"""

import os
import sys
import tempfile
import unittest

# Add scripts directory to path
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

from doc_tools.inventory import (
    ContractPolicy,
    PolicyContradictionError,
    PolicyError,
    UnclassifiedDocumentError,
    discover_documents,
)
from doc_tools.markdown import MarkdownDocument, slugify_heading
from docs import LinkChecker, normalize_github_link


class TestMarkdownParser(unittest.TestCase):
    def test_slugify_heading(self):
        self.assertEqual(slugify_heading("Main Heading"), "main-heading")
        self.assertEqual(slugify_heading("Heading `code` & *style*!"), "heading-code-style")
        self.assertEqual(slugify_heading("Unicode 日本語"), "unicode-日本語")
        self.assertEqual(slugify_heading("What's new?"), "whats-new")

    def test_duplicate_headings_and_anchors(self):
        content = """
# Section One
## Section One
## Section One
<a name="custom-anchor"></a>
<div id="div-anchor"></div>
"""
        doc = MarkdownDocument("test.md", content)
        expected_anchors = {"section-one", "section-one-1", "section-one-2", "custom-anchor", "div-anchor"}
        self.assertEqual(doc.anchors, expected_anchors)

    def test_code_fences_ignored(self):
        content = """
[Valid Link](./real_file.md)

```sh
# Code fence path should NOT be treated as a link
ls -la ./nonexistent_file.md
```
"""
        doc = MarkdownDocument("test.md", content)
        self.assertEqual(len(doc.links), 1)
        self.assertEqual(doc.links[0].raw_url, "./real_file.md")

    def test_crlf_line_endings(self):
        content = "# CRLF Heading\r\n\r\n[CRLF Link](./target.md)\r\n"
        doc = MarkdownDocument("test.md", content)
        self.assertIn("crlf-heading", doc.anchors)
        self.assertEqual(len(doc.links), 1)
        self.assertEqual(doc.links[0].raw_url, "./target.md")


class TestInventoryAndPolicy(unittest.TestCase):
    def setUp(self):
        self.fixture_dir = os.path.abspath(
            os.path.join(os.path.dirname(__file__), "..", "tests", "fixtures", "doc_tools", "valid")
        )

    def test_inventory_discovery_valid(self):
        contracts_path = os.path.join(self.fixture_dir, "contracts.toml")
        res = discover_documents(self.fixture_dir, contracts_path, mode="local")

        maintained_paths = [r.rel_path for r in res["maintained"]]
        self.assertIn("README.md", maintained_paths)
        self.assertIn("docs/guide.md", maintained_paths)
        self.assertIn("docs/file with spaces.md", maintained_paths)
        # Verify automatic discovery of nested readme without explicit path entry
        self.assertIn("docs/nested/readme.md", maintained_paths)

        aliases = [r.rel_path for r in res["canonical_aliases"]]
        self.assertIn("symlink_alias.md", aliases)

    def test_unclassified_document_error(self):
        with tempfile.TemporaryDirectory() as tmp:
            contracts_path = os.path.join(tmp, "contracts.toml")
            with open(contracts_path, "w") as f:
                f.write('version = 1\n[[rules]]\npath="README.md"\nclassification="maintained"\nreason="Root"\n')

            with open(os.path.join(tmp, "README.md"), "w") as f:
                f.write("# Root\n")

            # Create unclassified nested readme
            os.makedirs(os.path.join(tmp, "unclassified_dir"))
            with open(os.path.join(tmp, "unclassified_dir", "readme.md"), "w") as f:
                f.write("# Unclassified\n")

            with self.assertRaises(UnclassifiedDocumentError):
                discover_documents(tmp, contracts_path, mode="local")

    def test_policy_contradiction_error(self):
        with tempfile.TemporaryDirectory() as tmp:
            contracts_path = os.path.join(tmp, "contracts.toml")
            with open(contracts_path, "w") as f:
                f.write(
                    "version = 1\n"
                    '[[rules]]\npath="README.md"\nclassification="maintained"\nreason="Rule 1"\n'
                    '[[rules]]\npath="README.md"\nclassification="historical"\nreason="Rule 2"\n'
                )

            with self.assertRaises(PolicyContradictionError):
                ContractPolicy(contracts_path).classify("README.md")

    def test_malformed_policy_error(self):
        with tempfile.TemporaryDirectory() as tmp:
            contracts_path = os.path.join(tmp, "contracts.toml")
            with open(contracts_path, "w") as f:
                f.write("version = 99\n")  # Invalid version

            with self.assertRaises(PolicyError):
                ContractPolicy(contracts_path)


class TestLinkChecker(unittest.TestCase):
    def setUp(self):
        self.fixture_dir = os.path.abspath(
            os.path.join(os.path.dirname(__file__), "..", "tests", "fixtures", "doc_tools", "valid")
        )

    def test_valid_fixture_passes(self):
        contracts_path = os.path.join(self.fixture_dir, "contracts.toml")
        checker = LinkChecker(self.fixture_dir, contracts_path=contracts_path, mode="local")
        self.assertTrue(checker.check_links(), f"Expected valid fixtures to pass, got errors: {checker.errors}")

    def test_missing_link_target_fails(self):
        with tempfile.TemporaryDirectory() as tmp:
            contracts_path = os.path.join(tmp, "contracts.toml")
            with open(contracts_path, "w") as f:
                f.write('version = 1\n[[rules]]\npath="README.md"\nclassification="maintained"\nreason="Root"\n')

            readme_path = os.path.join(tmp, "README.md")
            with open(readme_path, "w") as f:
                f.write("# Title\n\n[Broken Link](./nonexistent_file.md)\n")

            checker = LinkChecker(tmp, contracts_path=contracts_path, mode="local")
            self.assertFalse(checker.check_links())
            self.assertTrue(any("broken link target" in err for err in checker.errors))
            self.assertTrue(any("README.md:3" in err for err in checker.errors))

    def test_missing_anchor_fails(self):
        with tempfile.TemporaryDirectory() as tmp:
            contracts_path = os.path.join(tmp, "contracts.toml")
            with open(contracts_path, "w") as f:
                f.write('version = 1\n[[rules]]\npath="README.md"\nclassification="maintained"\nreason="Root"\n')

            readme_path = os.path.join(tmp, "README.md")
            with open(readme_path, "w") as f:
                f.write("# Title\n\n[Broken Anchor](#nonexistent-anchor)\n")

            checker = LinkChecker(tmp, contracts_path=contracts_path, mode="local")
            self.assertFalse(checker.check_links())
            self.assertTrue(any("missing anchor '#nonexistent-anchor'" in err for err in checker.errors))

    def test_normalize_github_link(self):
        owner, frag = normalize_github_link(
            "https://github.com/d-o-hub/do-harness/blob/main/docs/cli.md#subcommand",
            root_dir=self.fixture_dir,
            repo_owner_name="d-o-hub/do-harness",
        )
        self.assertEqual(owner, "docs/cli.md")
        self.assertEqual(frag, "subcommand")


if __name__ == "__main__":
    unittest.main()
