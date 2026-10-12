#!/usr/bin/env python3
"""Regression tests for distribution validation's fail-closed provenance gate."""

import copy
import json
import locale
import os
import pathlib
import shutil
import tempfile
import unittest

from check_source_distribution import (
    ROOT, POLICY, check_db_manifest, check_fork, check_metadata, run,
)


class ProvenanceTests(unittest.TestCase):
    def setUp(self):
        self.scratch = tempfile.TemporaryDirectory()
        self.addCleanup(self.scratch.cleanup)
        self.root = pathlib.Path(self.scratch.name)
        self.policy = json.loads(POLICY.read_text(encoding="utf-8"))
        self.fork = self.root / "fork"
        (self.fork / "src/local").mkdir(parents=True)
        for relative in ("Cargo.toml", "LICENSE", "src/local/connection.rs"):
            shutil.copyfile(ROOT / "vendor/libsql" / relative, self.fork / relative)
        self.db = self.root / "db"
        self.db.mkdir()
        self.manifest = '''[package]
name = "do-harness-db"
version = "0.3.0"
[dependencies.libsql]
package = "do-harness-libsql"
version = "0.9.30"
default-features = false
features = ["core"]
'''
        (self.db / "Cargo.toml").write_text(self.manifest, encoding="utf-8")
        source = "registry+https://github.com/rust-lang/crates.io-index"
        self.metadata = {"packages": [
            {"id": "fork", "name": "do-harness-libsql", "version": "0.9.30",
             "source": source, "manifest_path": str(self.fork / "Cargo.toml")},
            {"id": "db", "name": "do-harness-db", "version": "0.3.0",
             "source": source, "manifest_path": str(self.db / "Cargo.toml")},
        ], "resolve": {"nodes": [{"id": "db", "deps": [{"name": "libsql", "pkg": "fork"}]}]}}

    def test_approved_registry_graph(self):
        check_metadata(self.metadata, self.policy, "0.3.0")

    def test_windows_line_endings_preserve_source_provenance(self):
        connection = self.fork / "src/local/connection.rs"
        source = connection.read_bytes().replace(b"\r\n", b"\n")
        connection.write_bytes(source.replace(b"\n", b"\r\n"))
        check_metadata(self.metadata, self.policy, "0.3.0")

    def test_shell_commands_are_rejected_before_execution(self):
        with self.assertRaisesRegex(ValueError, "only Cargo and rustc"):
            run(["bash", "-c", "exit 0"], self.root, {})

    def test_nul_arguments_are_rejected_before_execution(self):
        with self.assertRaisesRegex(ValueError, "NUL"):
            run(["cargo", "version\0"], self.root, {})

    def test_original_registry_dependency_rejected(self):
        (self.db / "Cargo.toml").write_text(self.manifest.replace('package = "do-harness-libsql"\n', ""))
        with self.assertRaises(ValueError):
            check_db_manifest(self.db, self.policy)

    def test_path_git_and_patch_workarounds_rejected(self):
        for suffix in ('path = "../fork"', 'git = "https://example.com/fork"',
                       '[patch.crates-io.libsql]\npath = "../fork"'):
            with self.subTest(suffix=suffix):
                (self.db / "Cargo.toml").write_text(self.manifest + suffix + "\n", encoding="utf-8")
                with self.assertRaises(ValueError):
                    check_db_manifest(self.db, self.policy)

    def test_checkout_dependency_is_not_registry_proof(self):
        self.metadata["packages"][0]["source"] = None
        with self.assertRaises(ValueError):
            check_metadata(self.metadata, self.policy, "0.3.0")

    def test_parallel_unpatched_implementation_rejected(self):
        self.metadata["packages"].append({"name": "libsql"})
        with self.assertRaises(ValueError):
            check_metadata(self.metadata, self.policy, "0.3.0")

    def test_dependency_edge_must_reach_fixed_fork(self):
        bad = copy.deepcopy(self.metadata)
        bad["resolve"]["nodes"][0]["deps"][0]["pkg"] = "some-other-package"
        with self.assertRaises(ValueError):
            check_metadata(bad, self.policy, "0.3.0")

    def test_lost_fix_is_rejected_even_with_correct_name_and_version(self):
        connection = self.fork / "src/local/connection.rs"
        connection.write_text(connection.read_text(encoding="utf-8").replace(
            "self.raw = std::ptr::null_mut();", "// lost the teardown fix"), encoding="utf-8")
        with self.assertRaisesRegex(ValueError, "digest changed"):
            check_fork(self.fork, self.policy)

    @unittest.skipIf(os.name != "posix", "POSIX locale semantics")
    def test_run_decodes_utf8_output_under_non_utf8_locale(self):
        """Cargo emits UTF-8; the host code page must not decide the decoding.

        `LC_ALL=C` in the child reproduces a Windows console, and the parent's
        LC_CTYPE is pinned to C for the duration of the call so the implicit
        `text=True` decoding uses a non-UTF-8 codec — the exact Windows failure
        (`UnicodeDecodeError: 'charmap' codec can't decode ...`) seen on
        `cargo metadata`. The previous locale is restored by cleanup.
        """
        try:
            previous = locale.setlocale(locale.LC_CTYPE)
            self.addCleanup(locale.setlocale, locale.LC_CTYPE, previous)
            locale.setlocale(locale.LC_CTYPE, "C")
        except locale.Error as error:  # pragma: no cover - exotic locale tables
            self.skipTest(f"C locale unavailable: {error}")
        if locale.getpreferredencoding(False).lower().replace("-", "") == "utf8":
            self.skipTest("UTF-8 mode always decodes as UTF-8")
        cargo = self.root / "cargo"
        cargo.write_text("#!/bin/sh\nprintf 'caf\\303\\251\\n'\n", encoding="utf-8")
        cargo.chmod(0o755)
        env = {**os.environ, "LC_ALL": "C", "PYTHONUTF8": "0",
               "PATH": f"{self.root}{os.pathsep}{os.environ['PATH']}"}
        output = run(["cargo", "metadata"], self.root, env, capture=True)
        self.assertIn("café", output)


if __name__ == "__main__":
    unittest.main()
