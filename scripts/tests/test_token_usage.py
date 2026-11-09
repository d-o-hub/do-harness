#!/usr/bin/env python3
"""Hermetic tests for scripts/token-usage.py.

Builds a temporary SQLite database with the same `session_v2` columns the
script reads, then asserts the summarisation and event payload without
touching a real OpenCode database.
"""

import importlib.util
import json
import sqlite3
import sys
import tempfile
import unittest
from pathlib import Path

SCRIPT = Path(__file__).resolve().parent.parent / "token-usage.py"


def load_module():
    spec = importlib.util.spec_from_file_location("token_usage", SCRIPT)
    module = importlib.util.module_from_spec(spec)
    sys.modules["token_usage"] = module
    spec.loader.exec_module(module)
    return module


def build_db(path: Path, rows):
    con = sqlite3.connect(path)
    con.execute(
        """
        CREATE TABLE session_v2 (
            id TEXT PRIMARY KEY, title TEXT, directory TEXT, model TEXT, agent TEXT,
            tokens_input INTEGER, tokens_output INTEGER, tokens_reasoning INTEGER,
            tokens_cache_read INTEGER, tokens_cache_write INTEGER, cost REAL,
            time_created INTEGER, time_updated INTEGER
        )
        """
    )
    con.executemany(
        "INSERT INTO session_v2 VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?)", rows
    )
    con.commit()
    con.close()


class TokenUsageTests(unittest.TestCase):
    def setUp(self):
        self.module = load_module()
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.db = Path(self.tmp.name) / "opencode.db"

    def test_reads_one_session_and_sums_usage(self):
        build_db(
            self.db,
            [
                (
                    "ses_a", "work", "/repo", "m", "build",
                    100, 40, 10, 5000, 700, 1.5, 1000, 2000,
                )
            ],
        )
        con = self.module.connect_readonly(self.db)
        rows = self.module.fetch_sessions(con, ["ses_a"], None, False, 1)
        con.close()
        report = self.module.summarise(rows)
        self.assertEqual(report["totals"]["input"], 100)
        self.assertEqual(report["totals"]["output"], 40)
        self.assertEqual(report["totals"]["total"], 140)
        self.assertEqual(report["totals"]["cache_read"], 5000)

    def test_cost_is_not_reported(self):
        """Events count tokens, not money: a cost field must not reappear."""
        build_db(
            self.db,
            [
                (
                    "ses_nopay", "t", "/repo", "m", "build",
                    10, 5, 0, 0, 0, 9.99, 1, 2,
                )
            ],
        )
        con = self.module.connect_readonly(self.db)
        report = self.module.summarise(
            self.module.fetch_sessions(con, ["ses_nopay"], None, False, 1)
        )
        con.close()
        self.assertNotIn("cost_usd", report["totals"])
        self.assertNotIn("cost_usd", report["sessions"][0]["usage"])
        payload = self.module.event_payload(report)
        self.assertNotIn("cost", json.dumps(payload).lower())

    def test_total_excludes_cache_and_reasoning(self):
        build_db(
            self.db,
            [
                (
                    "ses_b", None, "/repo", "m", "build",
                    10, 20, 15, 999999, 1, 0.0, 1, 2,
                )
            ],
        )
        con = self.module.connect_readonly(self.db)
        report = self.module.summarise(
            self.module.fetch_sessions(con, ["ses_b"], None, False, 1)
        )
        con.close()
        self.assertEqual(report["totals"]["total"], 30)
        self.assertEqual(report["totals"]["reasoning"], 15)

    def test_directory_filter_and_limit(self):
        build_db(
            self.db,
            [
                (f"ses_{i}", f"t{i}", "/repo", "m", "build", 1, 1, 0, 0, 0, 0.0, i, i)
                for i in range(1, 6)
            ]
            + [("ses_other", "x", "/elsewhere", "m", "build", 1, 1, 0, 0, 0, 0.0, 9, 9)],
        )
        con = self.module.connect_readonly(self.db)
        rows = self.module.fetch_sessions(con, [], "/repo", True, 3)
        con.close()
        self.assertEqual(len(rows), 3)
        self.assertTrue(all(r["directory"] == "/repo" for r in rows))
        # newest first
        self.assertEqual([r["id"] for r in rows], ["ses_5", "ses_4", "ses_3"])

    def test_missing_database_fails_loudly(self):
        with self.assertRaises(SystemExit):
            self.module.connect_readonly(Path(self.tmp.name) / "nope.db")

    def test_path_with_uri_special_characters_is_not_truncated(self):
        # A `?` or `#` in the directory must not turn into URI query/fragment.
        weird = Path(self.tmp.name) / "odd?dir#x"
        weird.mkdir()
        db = weird / "opencode.db"
        build_db(
            db,
            [("ses_odd", "t", "/repo", "m", "build", 2, 3, 0, 0, 0, 0.0, 1, 2)],
        )
        con = self.module.connect_readonly(db)
        rows = self.module.fetch_sessions(con, ["ses_odd"], None, False, 1)
        con.close()
        self.assertEqual(self.module.summarise(rows)["totals"]["total"], 5)

    def test_event_payload_is_serialisable_and_named(self):
        build_db(
            self.db,
            [("ses_c", "a titled run", "/repo", "m", "build", 5, 5, 0, 0, 0, 0.1, 1, 2)],
        )
        con = self.module.connect_readonly(self.db)
        report = self.module.summarise(
            self.module.fetch_sessions(con, ["ses_c"], None, False, 1)
        )
        con.close()
        payload = self.module.event_payload(report)
        self.assertEqual(payload["kind"], "metrics")
        self.assertIn("a titled run", payload["title"])
        json.dumps(payload)  # must be serialisable

    def test_report_only_writes_nothing(self):
        build_db(
            self.db,
            [("ses_d", "t", "/repo", "m", "build", 1, 1, 0, 0, 0, 0.0, 1, 2)],
        )
        out = Path(self.tmp.name) / "event.json"
        code = self.module.main(
            ["--db", str(self.db), "--session", "ses_d", "--report", "--out", str(out)]
        )
        self.assertEqual(code, 0)
        self.assertFalse(out.exists(), "--report must not write a file")

    def test_unknown_session_returns_nonzero(self):
        build_db(
            self.db,
            [("ses_e", "t", "/repo", "m", "build", 1, 1, 0, 0, 0, 0.0, 1, 2)],
        )
        code = self.module.main(["--db", str(self.db), "--session", "does-not-exist", "--report"])
        self.assertEqual(code, 1)


if __name__ == "__main__":
    unittest.main()
