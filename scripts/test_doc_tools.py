#!/usr/bin/env python3
"""
test_doc_tools.py — Unit and integration tests for inventory discovery, contract policy,
markdown parsing, and link verification.
"""

import os
import shutil
import subprocess
import sys
import tempfile
import unittest
from unittest import mock

# Add scripts directory to path
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

from doc_tools import cli_sync
from doc_tools.github_slugger import GithubSlugger
from doc_tools.inventory import (
    ContractPolicy,
    PolicyContradictionError,
    PolicyError,
    UnclassifiedDocumentError,
    _get_git_files,
    discover_documents,
)
from doc_tools.markdown import MarkdownDocument, slugify_heading
from docs import LinkChecker, main as docs_main, normalize_github_link

README_POLICY = 'version = 1\n[[rules]]\npath="README.md"\nclassification="maintained"\nreason="Root"\n'

DOCS_POLICY = (
    'version = 1\n[[rules]]\npath="README.md"\nclassification="maintained"\nreason="Root"\n'
    '[[rules]]\nglob="docs/*.md"\nclassification="maintained"\nreason="Docs"\n'
)


def write_file(path, content):
    with open(path, "w", encoding="utf-8") as f:
        f.write(content)


# The docs sensors run `python3 scripts/docs.py <check> --check` (do-harness.toml),
# so the can-fail tests below run that same process: the sensor runner reads the
# exit code, not the in-process checker verdict.
DOCS_CLI = os.path.abspath(os.path.join(os.path.dirname(__file__), "docs.py"))
REPO_ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), ".."))
FIXTURES_DIR = os.path.join(REPO_ROOT, "tests", "fixtures", "doc_tools")


def docs_fixture(name):
    """Absolute path of a fixture repository under tests/fixtures/doc_tools."""
    return os.path.join(FIXTURES_DIR, name)


def run_docs(args, root, env=None, python_flags=()):
    """Runs `python3 scripts/docs.py <args> --root <root>` the way a docs sensor does."""
    argv = [sys.executable, *python_flags, DOCS_CLI, *args, "--root", root]
    return subprocess.run(argv, capture_output=True, text=True, env=env)


class TestMarkdownParser(unittest.TestCase):
    def test_slugify_heading(self):
        # github-slugger parity: lowercase, delete the upstream remove set
        # (doc_tools/github_slugger.py), then map each U+0020 space to '-'.
        self.assertEqual(slugify_heading("Main Heading"), "main-heading")
        self.assertEqual(slugify_heading("Heading `code` & *style*!"), "heading-code--style")
        self.assertEqual(slugify_heading("Unicode 日本語"), "unicode-日本語")
        self.assertEqual(slugify_heading("What's new?"), "whats-new")
        self.assertEqual(slugify_heading("bravoCharlieDelta"), "bravocharliedelta")
        self.assertEqual(slugify_heading("I ♥ unicode"), "i--unicode")
        self.assertEqual(slugify_heading("😄 emoji"), "-emoji")
        self.assertEqual(slugify_heading("Привет non-latin 你好"), "привет-non-latin-你好")
        self.assertEqual(slugify_heading("en–dash"), "endash")
        self.assertEqual(slugify_heading("foo_bar"), "foo_bar")
        # Kept: superscripts are removed, but enclosing marks, combining marks,
        # and connector punctuation survive exactly as github-slugger leaves them.
        self.assertEqual(slugify_heading("a²b"), "ab")
        self.assertEqual(slugify_heading("aⓔb"), "aⓔb")
        self.assertEqual(slugify_heading("a\u0300b"), "a\u0300b")
        self.assertEqual(slugify_heading("a\u203fb"), "a\u203fb")
        # Zero-width joiner is in the remove set.
        self.assertEqual(slugify_heading("a\u200db"), "ab")

    def test_slugger_repetition_sequence(self):
        # Mirrors upstream fixture "Repetition (1..5)": the occurrence map is
        # keyed by slug, so a claimed "echo-1" pushes the next duplicate to
        # "echo-2" while heading text "echo 1" claims "echo-1-1".
        slugger = GithubSlugger()
        self.assertEqual(
            [slugger.slug(v) for v in ["echo", "echo", "echo 1", "echo-1", "echo"]],
            ["echo", "echo-1", "echo-1-1", "echo-1-2", "echo-2"],
        )
        slugger = GithubSlugger()
        self.assertEqual(
            [slugger.slug(v) for v in ["foxtrot-1", "foxtrot", "foxtrot"]],
            ["foxtrot-1", "foxtrot", "foxtrot-2"],
        )

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

    def test_anchors_ignore_prose_code_and_comments(self):
        content = (
            "# Heading\n\n"
            '<span data-id="not-an-anchor"></span>\n'
            '`<span id="in-code"></span>`\n'
            '<!-- <span id="in-comment"></span> -->\n'
            '<span id="real-anchor"></span>\n'
        )
        doc = MarkdownDocument("test.md", content)
        self.assertIn("real-anchor", doc.anchors)
        self.assertNotIn("not-an-anchor", doc.anchors)
        self.assertNotIn("in-code", doc.anchors)
        self.assertNotIn("in-comment", doc.anchors)

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
        # The alias lives in a subdirectory so the distribution-context check
        # (relative links must not resolve from the alias directory) has teeth.
        self.assertIn("alias/symlink_alias.md", aliases)

    def test_unclassified_document_error(self):
        with tempfile.TemporaryDirectory() as tmp:
            contracts_path = os.path.join(tmp, "contracts.toml")
            write_file(contracts_path, README_POLICY)
            write_file(os.path.join(tmp, "README.md"), "# Root\n")

            # Create unclassified nested readme
            os.makedirs(os.path.join(tmp, "unclassified_dir"))
            write_file(os.path.join(tmp, "unclassified_dir", "readme.md"), "# Unclassified\n")

            with self.assertRaises(UnclassifiedDocumentError):
                discover_documents(tmp, contracts_path, mode="local")

    def test_policy_contradiction_error(self):
        with tempfile.TemporaryDirectory() as tmp:
            contracts_path = os.path.join(tmp, "contracts.toml")
            write_file(
                contracts_path,
                "version = 1\n"
                '[[rules]]\npath="README.md"\nclassification="maintained"\nreason="Rule 1"\n'
                '[[rules]]\npath="README.md"\nclassification="historical"\nreason="Rule 2"\n',
            )

            with self.assertRaises(PolicyContradictionError):
                ContractPolicy(contracts_path).classify("README.md")

    def test_malformed_policy_error(self):
        with tempfile.TemporaryDirectory() as tmp:
            contracts_path = os.path.join(tmp, "contracts.toml")
            write_file(contracts_path, "version = 99\n")  # Invalid version

            with self.assertRaises(PolicyError):
                ContractPolicy(contracts_path)


class TestLocalDiscoveryOfUntrackedDocuments(unittest.TestCase):
    """`local` discovery must see untracked-but-not-ignored documents.

    CI checks a fully committed tree, but the pre-commit half of the contract
    checks the working tree before a document is staged. On git 2.53
    `git ls-files --others --exclude-standard ':(icase)*.md'` returns nothing
    when the checkout root is a nested directory that has no entries in the
    index — the shape `docs.py --root tests/fixtures/<fixture>` uses — so the
    untracked half of discovery must not depend on that pathspec.
    """

    def setUp(self):
        if shutil.which("git") is None:
            self.skipTest("without git, local discovery falls back to a filesystem walk")
        # Isolating the git environment keeps the host's global/system config
        # (an excludesfile, for instance) out of the temp repository's ignore
        # decisions, so the fixture alone decides what is discovered.
        self.git_env = dict(
            os.environ,
            GIT_CONFIG_NOSYSTEM="1",
            GIT_CONFIG_GLOBAL=os.devnull,
            GIT_CONFIG_SYSTEM=os.devnull,
        )
        tmp = tempfile.TemporaryDirectory()
        self.addCleanup(tmp.cleanup)
        self.repo = os.path.join(tmp.name, "repo")
        os.makedirs(self.repo)
        self.run_git("init", "-q")

    def run_git(self, *args):
        return subprocess.run(
            ["git", "-C", self.repo, *args],
            check=True,
            capture_output=True,
            text=True,
            env=self.git_env,
        )

    def stage(self, rel_path, content):
        """Writes a file under the temp repo and adds it to the index (no commit)."""
        path = os.path.join(self.repo, rel_path)
        os.makedirs(os.path.dirname(path), exist_ok=True)
        write_file(path, content)
        self.run_git("add", rel_path)

    def make_nested_fixture(self):
        """A nested, untracked-only document tree, like a `--root` fixture."""
        root = os.path.join(self.repo, "fixtures", "nested")
        docs = os.path.join(root, "docs")
        os.makedirs(docs)
        write_file(os.path.join(docs, "pending.md"), "# Pending\n")
        write_file(os.path.join(docs, ".gitignore"), "ignored.md\n")
        write_file(os.path.join(docs, "ignored.md"), "# Ignored\n")
        write_file(os.path.join(root, "contracts.toml"), DOCS_POLICY)
        return root

    def test_untracked_document_discovered_and_ignored_excluded(self):
        root = self.make_nested_fixture()
        with mock.patch.dict(os.environ, self.git_env):
            files = _get_git_files(root, mode="local")
            discovered = discover_documents(
                root, os.path.join(root, "contracts.toml"), mode="local"
            )
        self.assertIn("docs/pending.md", files)
        self.assertNotIn("docs/ignored.md", files)
        maintained = [r.rel_path for r in discovered["maintained"]]
        self.assertIn("docs/pending.md", maintained)
        self.assertNotIn("docs/ignored.md", maintained)

    def test_tracked_documents_still_discovered(self):
        self.make_nested_fixture()
        self.stage("README.md", "# Root\n")
        with mock.patch.dict(os.environ, self.git_env):
            files = _get_git_files(self.repo, mode="local")
        self.assertIn("README.md", files)  # tracked half unchanged
        self.assertIn("fixtures/nested/docs/pending.md", files)
        self.assertNotIn("fixtures/nested/docs/ignored.md", files)


class TestLinkChecker(unittest.TestCase):
    def setUp(self):
        self.fixture_dir = os.path.abspath(
            os.path.join(os.path.dirname(__file__), "..", "tests", "fixtures", "doc_tools", "valid")
        )

    def test_valid_fixture_passes(self):
        contracts_path = os.path.join(self.fixture_dir, "contracts.toml")
        checker = LinkChecker(self.fixture_dir, contracts_path=contracts_path, mode="local")
        self.assertTrue(checker.check_links(), f"Expected valid fixtures to pass, got errors: {checker.errors}")

    def test_alias_in_subdirectory_warns_only_for_relative_links(self):
        contracts_path = os.path.join(self.fixture_dir, "contracts.toml")
        checker = LinkChecker(self.fixture_dir, contracts_path=contracts_path, mode="local")
        self.assertTrue(checker.check_links(), f"Expected valid fixtures to pass, got errors: {checker.errors}")

        # `alias/symlink_alias.md` sits in a subdirectory, so the canonical
        # README's relative links cannot resolve from the alias directory: one
        # notice per relative link (guide, space filename, reference link,
        # image). The absolute GitHub URL resolves identically from anywhere
        # and must not warn.
        self.assertEqual(len(checker.warnings), 4, checker.warnings)
        self.assertTrue(all("alias/symlink_alias.md" in w for w in checker.warnings), checker.warnings)
        self.assertTrue(any("docs/guide.md" in w for w in checker.warnings), checker.warnings)
        self.assertTrue(any("file with spaces.md" in w for w in checker.warnings), checker.warnings)

    def test_missing_link_target_fails(self):
        with tempfile.TemporaryDirectory() as tmp:
            contracts_path = os.path.join(tmp, "contracts.toml")
            write_file(contracts_path, README_POLICY)
            write_file(os.path.join(tmp, "README.md"), "# Title\n\n[Broken Link](./nonexistent_file.md)\n")

            checker = LinkChecker(tmp, contracts_path=contracts_path, mode="local")
            self.assertFalse(checker.check_links())
            self.assertTrue(any("broken link target" in err for err in checker.errors))
            self.assertTrue(any("README.md:3" in err for err in checker.errors))

    def test_missing_anchor_fails(self):
        with tempfile.TemporaryDirectory() as tmp:
            contracts_path = os.path.join(tmp, "contracts.toml")
            write_file(contracts_path, README_POLICY)
            write_file(os.path.join(tmp, "README.md"), "# Title\n\n[Broken Anchor](#nonexistent-anchor)\n")

            checker = LinkChecker(tmp, contracts_path=contracts_path, mode="local")
            self.assertFalse(checker.check_links())
            self.assertTrue(any("missing anchor '#nonexistent-anchor'" in err for err in checker.errors))

    def test_anchor_top_and_percent_encoding(self):
        with tempfile.TemporaryDirectory() as tmp:
            os.makedirs(os.path.join(tmp, "docs"))
            write_file(os.path.join(tmp, "contracts.toml"), DOCS_POLICY)
            write_file(
                os.path.join(tmp, "README.md"),
                "# Title\n\n[Top](#top)\n[Encoded](./docs/guide.md#caf%C3%A9)\n",
            )
            write_file(os.path.join(tmp, "docs", "guide.md"), "# Café\n")

            checker = LinkChecker(tmp, contracts_path=os.path.join(tmp, "contracts.toml"), mode="local")
            self.assertTrue(checker.check_links(), checker.errors)
            self.assertEqual(checker.errors, [])

    def test_network_and_site_absolute_links_are_skipped(self):
        with tempfile.TemporaryDirectory() as tmp:
            write_file(os.path.join(tmp, "contracts.toml"), README_POLICY)
            write_file(
                os.path.join(tmp, "README.md"),
                "# Title\n\n"
                "[tel](tel:+1-234-567)\n"
                "[data](data:text/plain,hi)\n"
                "[root](/etc/hosts)\n"
                "[proto](//example.com/x.md)\n"
                "[query](./real.md?plain=1)\n",
            )
            write_file(os.path.join(tmp, "real.md"), "# Real\n")

            checker = LinkChecker(tmp, contracts_path=os.path.join(tmp, "contracts.toml"), mode="local")
            self.assertTrue(checker.check_links(), checker.errors)
            self.assertEqual(checker.errors, [])

    def test_query_string_is_removed_before_resolution(self):
        with tempfile.TemporaryDirectory() as tmp:
            write_file(os.path.join(tmp, "contracts.toml"), README_POLICY)
            write_file(os.path.join(tmp, "README.md"), "# Title\n\n[Broken](./missing.md?plain=1)\n")

            checker = LinkChecker(tmp, contracts_path=os.path.join(tmp, "contracts.toml"), mode="local")
            self.assertFalse(checker.check_links())
            self.assertTrue(
                any("missing.md" in err and "?plain=1" not in err for err in checker.errors), checker.errors
            )

    def test_link_escaping_repository_root_is_rejected(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = os.path.join(tmp, "checkout")
            os.makedirs(root)
            write_file(os.path.join(root, "contracts.toml"), README_POLICY)
            write_file(os.path.join(root, "README.md"), "# Title\n\n[Escape](../outside.md)\n")
            # The target exists, but only outside the checkout root: existence
            # alone is not a valid resolution.
            write_file(os.path.join(tmp, "outside.md"), "# Outside\n")

            checker = LinkChecker(root, contracts_path=os.path.join(root, "contracts.toml"), mode="local")
            self.assertFalse(checker.check_links())
            self.assertTrue(any("escapes the repository root" in err for err in checker.errors), checker.errors)

    def test_directory_target_is_accepted(self):
        # GitHub serves `/blob/<dir>` by redirecting to `/tree/<dir>` (verified
        # live), and existence is the documented target test
        # (docs/documentation-contract.md), so an existing directory is a valid
        # link target.
        with tempfile.TemporaryDirectory() as tmp:
            os.makedirs(os.path.join(tmp, "docs"))
            write_file(os.path.join(tmp, "contracts.toml"), README_POLICY)
            write_file(os.path.join(tmp, "README.md"), "# Title\n\n[Dir](./docs)\n")

            checker = LinkChecker(tmp, contracts_path=os.path.join(tmp, "contracts.toml"), mode="local")
            self.assertTrue(checker.check_links(), checker.errors)
            self.assertEqual(checker.errors, [])

    def test_normalize_github_link(self):
        owner, frag = normalize_github_link(
            "https://github.com/d-o-hub/do-harness/blob/main/docs/cli.md#subcommand",
            root_dir=self.fixture_dir,
            repo_owner_name="d-o-hub/do-harness",
        )
        self.assertEqual(owner, "docs/cli.md")
        self.assertEqual(frag, "subcommand")

    def test_normalize_foreign_github_link_is_not_resolved_locally(self):
        # The fixture directory contains README.md — the exact path this link
        # names — but a different owner/repo proves it is not this checkout's
        # file, so it must not be verified against the local tree.
        path, frag = normalize_github_link(
            "https://github.com/other-org/other-repo/blob/main/README.md#x",
            root_dir=self.fixture_dir,
            repo_owner_name="d-o-hub/do-harness",
        )
        self.assertIsNone(path)
        self.assertIsNone(frag)

    def test_normalize_without_origin_falls_back_to_local_existence(self):
        path, frag = normalize_github_link(
            "https://github.com/any-org/any-repo/blob/main/README.md#x",
            root_dir=self.fixture_dir,
            repo_owner_name=None,
        )
        self.assertEqual(path, "README.md")
        self.assertEqual(frag, "x")

    def test_foreign_github_link_is_skipped_not_verified(self):
        with tempfile.TemporaryDirectory() as tmp:
            write_file(os.path.join(tmp, "contracts.toml"), README_POLICY)
            write_file(
                os.path.join(tmp, "README.md"),
                "# Title\n\n[Foreign](https://github.com/other-org/other-repo/blob/main/README.md#missing-anchor)\n",
            )

            checker = LinkChecker(tmp, contracts_path=os.path.join(tmp, "contracts.toml"), mode="local")
            # Simulate a known origin whose owner/repo differs from the link.
            checker.repo_owner_name = "d-o-hub/do-harness"
            self.assertTrue(checker.check_links(), checker.errors)
            self.assertEqual(checker.errors, [])


class TestCliSync(unittest.TestCase):
    def test_check_mode_invokes_fixed_cargo_argv(self):
        completed = mock.Mock(returncode=0)
        with mock.patch.object(cli_sync.subprocess, "run", return_value=completed) as run:
            code = cli_sync.run_sync("/repo", write=False, env={})
        self.assertEqual(code, 0)
        self.assertEqual(
            run.call_args.args[0],
            [
                "cargo",
                "test",
                "-p",
                "do-harness",
                "--bin",
                "do-harness",
                "--quiet",
                cli_sync.TEST_FILTER,
            ],
        )
        self.assertEqual(run.call_args.kwargs["cwd"], "/repo")
        self.assertEqual(run.call_args.kwargs["check"], False)
        self.assertNotIn(cli_sync.WRITE_ENV, run.call_args.kwargs["env"])

    def test_write_mode_sets_update_env(self):
        completed = mock.Mock(returncode=0)
        with mock.patch.object(cli_sync.subprocess, "run", return_value=completed) as run:
            code = cli_sync.run_sync("/repo", write=True, env={"PATH": "/usr/bin"})
        self.assertEqual(code, 0)
        self.assertEqual(run.call_args.kwargs["env"][cli_sync.WRITE_ENV], "1")

    def test_write_mode_is_refused_in_ci(self):
        with mock.patch.object(cli_sync.subprocess, "run") as run:
            code = cli_sync.run_sync("/repo", write=True, env={"CI": "true"})
        self.assertEqual(code, 2)
        run.assert_not_called()

    def test_ci_detection_matches_common_spellings(self):
        self.assertFalse(cli_sync.ci_is_set({}))
        self.assertFalse(cli_sync.ci_is_set({"CI": ""}))
        self.assertFalse(cli_sync.ci_is_set({"CI": "0"}))
        self.assertFalse(cli_sync.ci_is_set({"CI": "false"}))
        self.assertTrue(cli_sync.ci_is_set({"CI": "true"}))

    def test_missing_cargo_returns_usage_error(self):
        with mock.patch.object(cli_sync.subprocess, "run", side_effect=FileNotFoundError):
            code = cli_sync.run_sync("/repo", write=False, env={})
        self.assertEqual(code, 2)

    def test_failing_test_exit_code_is_propagated(self):
        completed = mock.Mock(returncode=101)
        with mock.patch.object(cli_sync.subprocess, "run", return_value=completed):
            code = cli_sync.run_sync("/repo", write=False, env={})
        self.assertEqual(code, 101)

    def test_check_and_write_flags_are_mutually_exclusive(self):
        with mock.patch.object(sys, "argv", ["docs.py", "sync", "--check", "--write"]):
            with self.assertRaises(SystemExit) as caught:
                docs_main()
        self.assertEqual(caught.exception.code, 2)


class TestDocsChecksCanFail(unittest.TestCase):
    """Can-fail proofs: every docs check must reject its committed bad input.

    Each case runs `python3 scripts/docs.py <check> --check --root <fixture>`
    exactly as the `docs-sync`/`docs-examples`/`docs-links` sensors do, against
    an isolated copy of a committed fixture repository, and asserts the
    non-zero exit that makes the sensor RED. A checker that regressed to
    accepting the bad input would exit 0 instead — the one outcome `verify`
    reports as a passing (or `SKIP:`-warned) sensor.
    """

    def copy_fixture(self, name):
        """Copies tests/fixtures/doc_tools/<name> into a fresh temporary root.

        The copy keeps the run hermetic (offline, no network) and independent
        of this checkout: `docs.py` walks the filesystem when its root is not a
        git checkout, so the committed fixture content is exactly what the
        check sees, staged or not.
        """
        tmp = tempfile.TemporaryDirectory()
        self.addCleanup(tmp.cleanup)
        return shutil.copytree(docs_fixture(name), os.path.join(tmp.name, name))

    def assert_check_failed(self, result, *needles):
        output = result.stdout + result.stderr
        self.assertNotEqual(result.returncode, 0, f"check exited 0: {output}")
        for needle in needles:
            self.assertIn(needle, output)
        # A zero exit carrying a `SKIP:` line still counts as a green sensor (it
        # is warned, not failed), so the marker must never appear either.
        self.assertNotIn("SKIP:", output)

    def test_broken_paths_and_anchors_fail_links(self):
        # No same-document fragment is broken here: markdownlint's MD051 owns
        # those, so the fixture carries the two failures this checker owns.
        result = run_docs(["links", "--check"], self.copy_fixture("invalid-links"))
        self.assert_check_failed(
            result,
            "broken link target './docs/absent.md'",
            "missing anchor '#absent-heading' in docs/guide.md",
            "Link check FAILED: 2 error(s) found.",
        )

    def test_invalid_executable_example_fails_examples(self):
        result = run_docs(["examples", "--check"], self.copy_fixture("invalid-examples"))
        self.assert_check_failed(
            result,
            "Example 'broken_example' references unknown fixture 'absent_fixture' in manifest",
        )

    def test_empty_discovery_fails_every_check(self):
        root = self.copy_fixture("empty")  # a policy, and no documents at all
        for args in (["links", "--check"], ["examples", "--check"]):
            with self.subTest(check=args[0]):
                self.assert_check_failed(
                    run_docs(args, root), "No documentation files were discovered."
                )

    @unittest.skipUnless(os.name == "posix", "the fixture cargo stand-in is a bash script")
    def test_malformed_generated_blocks_fail_sync_check(self):
        root = self.copy_fixture("malformed-generated")
        # The fixture really carries unterminated marker pairs, so "repairing"
        # it cannot silently turn this proof into a no-op.
        for name in ("README.md", "docs/cli.md"):
            with open(os.path.join(root, name), encoding="utf-8") as handle:
                text = handle.read()
            self.assertEqual(
                text.count("BEGIN GENERATED"), text.count("END GENERATED") + 1, name
            )

        # `sync` rides on the crate's maintenance test through `cargo`; the
        # fixture `bin/cargo` stands in for `cargo test` and reports that test's
        # verdict for the artifacts above (101 on an unterminated pair). A
        # `PATH` holding only the stand-in keeps this hermetic: the host's
        # `cargo` never runs.
        env = dict(os.environ, PATH=os.path.join(root, "bin"))
        result = run_docs(["sync", "--check"], root, env=env)
        self.assert_check_failed(
            result, "generated documentation is out of date", "marker pair has no END"
        )
        self.assertEqual(result.returncode, 101, result.stderr)

    def test_missing_dependencies_fail_closed(self):
        """An absent dependency fails the check; it never skips it green.

        `scripts/docs.py` has no `SKIP:` path, so a check that cannot run exits
        non-zero under `CI=true` and `DO_HARNESS_REQUIRE_TOOLS=1` — and locally
        too, where there is likewise nothing to fall back to.
        """
        root = self.copy_fixture("invalid-links")
        with tempfile.TemporaryDirectory() as empty_bin:
            for label, extra in (
                ("local", {}),
                ("CI=true", {"CI": "true"}),
                ("DO_HARNESS_REQUIRE_TOOLS=1", {"DO_HARNESS_REQUIRE_TOOLS": "1"}),
            ):
                with self.subTest(scenario=label, dependency="markdown-it-py"):
                    # `-S` drops site-packages, which is exactly the parser
                    # scripts/requirements-docs.txt pins being absent.
                    result = run_docs(
                        ["links", "--check"], root, env=dict(os.environ, **extra), python_flags=("-S",)
                    )
                    self.assert_check_failed(result, "ModuleNotFoundError", "markdown_it")

                with self.subTest(scenario=label, dependency="cargo"):
                    # `docs-sync` needs the Rust toolchain to reach the
                    # maintenance test; an empty `PATH` is a host without it.
                    result = run_docs(
                        ["sync", "--check"], root, env=dict(os.environ, PATH=empty_bin, **extra)
                    )
                    self.assert_check_failed(result, "cargo not found on PATH")
                    self.assertEqual(result.returncode, 2, result.stderr)

                with self.subTest(scenario=label, dependency="python3"):
                    # The interpreter itself absent: the sensor command cannot
                    # start, so there is no exit code that could report a pass —
                    # the runner records the failed spawn.
                    with self.assertRaises(FileNotFoundError):
                        subprocess.run(
                            ["python3", DOCS_CLI, "links", "--check", "--root", root],
                            capture_output=True,
                            env=dict(os.environ, PATH=empty_bin, **extra),
                        )


if __name__ == "__main__":
    unittest.main()
