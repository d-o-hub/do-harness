#!/usr/bin/env python3
"""Regression tests for distribution validation's fail-closed provenance gate."""

import copy
import json
import pathlib
import shutil
import tempfile
import unittest

from check_source_distribution import (
    ROOT, POLICY, check_db_manifest, check_fork, check_metadata,
)


class ProvenanceTests(unittest.TestCase):
    def setUp(self):
        self.scratch = tempfile.TemporaryDirectory()
        self.addCleanup(self.scratch.cleanup)
        self.root = pathlib.Path(self.scratch.name)
        self.policy = json.loads(POLICY.read_text())
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
        (self.db / "Cargo.toml").write_text(self.manifest)
        source = "registry+https://github.com/rust-lang/crates.io-index"
        self.metadata = {"packages": [
            {"id": "fork", "name": "do-harness-libsql", "version": "0.9.30",
             "source": source, "manifest_path": str(self.fork / "Cargo.toml")},
            {"id": "db", "name": "do-harness-db", "version": "0.3.0",
             "source": source, "manifest_path": str(self.db / "Cargo.toml")},
        ], "resolve": {"nodes": [{"id": "db", "deps": [{"name": "libsql", "pkg": "fork"}]}]}}

    def test_approved_registry_graph(self):
        check_metadata(self.metadata, self.policy, "0.3.0")

    def test_original_registry_dependency_rejected(self):
        (self.db / "Cargo.toml").write_text(self.manifest.replace('package = "do-harness-libsql"\n', ""))
        with self.assertRaises(ValueError):
            check_db_manifest(self.db, self.policy)

    def test_path_git_and_patch_workarounds_rejected(self):
        for suffix in ('path = "../fork"', 'git = "https://example.com/fork"',
                       '[patch.crates-io.libsql]\npath = "../fork"'):
            with self.subTest(suffix=suffix):
                (self.db / "Cargo.toml").write_text(self.manifest + suffix + "\n")
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
        connection.write_text(connection.read_text().replace(
            "self.raw = std::ptr::null_mut();", "// lost the teardown fix"))
        with self.assertRaisesRegex(ValueError, "digest changed"):
            check_fork(self.fork, self.policy)


if __name__ == "__main__":
    unittest.main()
