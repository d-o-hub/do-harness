"""Deterministic self-tests for the #499 decision machinery.

Covers corpus validation, oracle-leak rejection, record validation,
adjudication, and the GO / NO_GO / INCONCLUSIVE verdict paths with synthetic
run records. No model or network is involved.
"""

import copy
import json
import tempfile
import unittest
from pathlib import Path

from . import corpus as corpus_mod
from . import record as record_mod
from .adjudicate import adjudicate_run
from .aggregate import (
    aggregate,
    load_protocol,
    validate_protocol,
    wilson_interval,
)
from .corpus import load_corpus, packet_contains_oracle

ROOT = Path(__file__).resolve().parents[2]
CORPUS_DIR = ROOT / "benchmarks" / "fresh-context-decision-v1"
PROTOCOL_FILE = CORPUS_DIR / "protocol.json"

FLAWED = ("authz-group-delegation-flawed", "migration-retention-backfill-flawed",
          "api-defaults-compat-flawed")
SOUND = ("authz-group-delegation-sound", "migration-retention-backfill-sound",
         "api-defaults-compat-sound")


def pinned_protocol():
    protocol = json.loads(PROTOCOL_FILE.read_text(encoding="utf-8"))
    protocol = copy.deepcopy(protocol)
    protocol["status"] = "pinned"
    protocol["pinned_at"] = "2026-11-21T00:00:00Z"
    protocol["pinned_from"] = "pilot-feasibility-placeholder"
    protocol["sample"]["repeated_runs_per_design_per_arm"] = 1
    protocol["sample"]["min_complete_designs"] = 6
    protocol["thresholds"]["mwi_advantage_rate"] = 0.5
    protocol["thresholds"]["false_positive_tolerance_per_design"] = 1.0
    protocol["thresholds"]["regression_tolerance_rate"] = 0.25
    protocol["thresholds"]["max_review_time_minutes"] = 15.0
    protocol["thresholds"]["max_usage_cost_usd"] = 5.0
    return protocol


def finding(fid, contract="acl.py:grant_delegation"):
    return {"id": fid, "contract_or_path": contract, "trigger": "trigger",
            "consequence": "consequence", "evidence": "evidence",
            "smallest_correction": "correction"}


def disposition(fid, status="confirmed", defect_id=None, duplicate_of=None):
    out = {"id": fid, "status": status}
    if defect_id is not None:
        out["defect_id"] = defect_id
    if duplicate_of is not None:
        out["duplicate_of"] = duplicate_of
    return out


def make_record(case_id, arm, *, findings=None, dispositions=None,
                outcome="success_with_findings", ran=True, success=True,
                regression=False, repair=0, verify=1, escaped=(),
                review_ms=120000, total_ms=300000, provenance="adapter-reported",
                coverage="full", currency=0.03, isolation="verified",
                pair=0, order=0):
    review = {
        "outcome": outcome,
        "packet_hash": "sha256:" + "ab" * 32,
        "findings": findings or [],
        "dispositions": dispositions or [],
        "limits": {"timeout_s": 300, "max_findings": 5,
                   "max_output_bytes": 65536},
        "isolation": {"marker_probe_configured": True,
                      "marker_leaked": False},
    }
    return {
        "schema_version": corpus_mod.SCHEMA_VERSION,
        "experiment": corpus_mod.EXPERIMENT,
        "record_type": "run",
        "case_id": case_id,
        "arm": arm,
        "pair_index": pair,
        "order_in_pair": order,
        "run_id": "%s-%s-%d" % (case_id, arm, pair),
        "review": review,
        "implementation": {
            "ran": ran,
            "final_task_success": success,
            "escaped_defects": list(escaped),
            "repair_cycles": repair,
            "regression_seen": regression,
            "verification_cost": verify,
        },
        "timing_ms": {
            "packet_prep": 1000, "reviewer_startup": 2000, "review": review_ms,
            "disposition": 3000, "fixes": 0, "implementation": 0,
            "verification": verify * 1000, "adjudication": 2000,
            "total": total_ms,
        },
        "usage": {
            "token_cost": 8000,  # nosec B105 -- synthetic usage count, not a credential
            "currency_cost": currency,
            "context_bytes": 200000,
            "provenance": provenance,
            "coverage": coverage,
            "usage_reason": None,
        },
        "identities": {
            "repo_commit": "0" * 40,
            "model_version": "fixture-model-v1",
            "adapter": "deterministic-fixture",
            "client_version": "fixture-client-v1",
            "review_completion": "completed",
            "isolation_evidence": isolation,
        },
    }


def six_design_records(treatment_caught=True, isolation="verified"):
    """Complete 6-design sample: 3 flawed + 3 sound, one run per arm."""
    records = []
    for case_id in FLAWED:
        caught = ("D1", "D2") if treatment_caught else ()
        findings = [finding(d) for d in caught]
        dispositions = [disposition(d, defect_id=d) for d in caught]
        records.append(make_record(case_id, "baseline"))
        records.append(make_record(case_id, "treatment", findings=findings,
                                   dispositions=dispositions,
                                   isolation=isolation))
    for case_id in SOUND:
        records.append(make_record(case_id, "baseline",
                                   outcome="success_zero_findings"))
        records.append(make_record(case_id, "treatment",
                                   outcome="success_zero_findings",
                                   isolation=isolation))
    return records


class CorpusTests(unittest.TestCase):
    def test_corpus_covers_risk_classes_and_variants(self):
        data = load_corpus(CORPUS_DIR)
        cases = data["cases"]
        self.assertEqual(len(cases), 7)
        classes = {c["class"] for c in cases}
        self.assertEqual(classes, {"authorization", "migration",
                                   "api-compat", "routine-control"})
        for risk in ("authorization", "migration", "api-compat"):
            variants = {c["variant"] for c in cases if c["class"] == risk}
            self.assertEqual(variants, {"flawed", "sound"})
        control = [c for c in cases if c["class"] == "routine-control"]
        self.assertEqual(len(control), 1)
        self.assertEqual(control[0]["variant"], "control")

    def test_every_case_predeclares_expected_findings(self):
        data = load_corpus(CORPUS_DIR)
        for case in data["cases"]:
            oracle = data["oracles"][case["id"]]
            expected = oracle["predeclared_expected_findings"]
            defect_ids = [d["id"] for d in oracle["defects"]]
            for fid in expected:
                self.assertIn(fid, defect_ids)
            if case["variant"] == "flawed":
                self.assertTrue(expected, case["id"])
            else:
                self.assertEqual(expected, [], case["id"])

    def test_packets_never_contain_oracle_material(self):
        data = load_corpus(CORPUS_DIR)
        for case in data["cases"]:
            packet = corpus_mod.build_packet(
                case, data["root"], review_id="self-test:%s" % case["id"]
            )
            self.assertFalse(packet_contains_oracle(packet), case["id"])
            self.assertNotIn("predeclared_expected_findings",
                             json.dumps(packet))
            self.assertNotIn(corpus_mod.ORACLE_FILENAME,
                             json.dumps(packet))

    def test_oracle_reference_is_rejected_by_packet_assembly(self):
        data = load_corpus(CORPUS_DIR)
        case = copy.deepcopy(data["cases"][0])
        case["design_doc"] = case["id"] + "/" + corpus_mod.ORACLE_FILENAME
        with self.assertRaises(ValueError):
            corpus_mod.build_packet(case, data["root"],
                                    review_id="self-test-leak")
        case = copy.deepcopy(data["cases"][0])
        case["source_files"] = [case["id"] + "/" + corpus_mod.ORACLE_FILENAME]
        with self.assertRaises(ValueError):
            corpus_mod.build_packet(case, data["root"],
                                    review_id="self-test-leak")

    def test_corpus_validation_rejects_oracle_reference(self):
        data = load_corpus(CORPUS_DIR)
        case = copy.deepcopy(data["cases"][0])
        case["source_files"] = [case["id"] + "/" + corpus_mod.ORACLE_FILENAME]
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "cases.json").write_text(
                json.dumps({"schema_version": 1,
                            "experiment": corpus_mod.EXPERIMENT,
                            "cases": [case]}, sort_keys=True),
                encoding="utf-8",
            )
            with self.assertRaises(ValueError):
                corpus_mod.load_cases(root)

    def test_protocol_sidecar_is_unpinned_placeholder(self):
        protocol = load_protocol(PROTOCOL_FILE)
        self.assertEqual(protocol["status"], "unpinned")
        self.assertIsNone(protocol["thresholds"]["mwi_advantage_rate"])
        self.assertIsNone(protocol["sample"]["min_complete_designs"])


class RecordTests(unittest.TestCase):
    def test_valid_record_accepts(self):
        record_mod.validate_run_record(make_record(FLAWED[0], "treatment"))

    def test_malformed_records_are_rejected(self):
        bad = make_record(FLAWED[0], "treatment")
        bad["usage"]["provenance"] = "invented"
        with self.assertRaises(ValueError):
            record_mod.validate_run_record(bad)
        bad = make_record(FLAWED[0], "treatment")
        bad["review"]["findings"] = [finding("F1"), finding("F1")]
        with self.assertRaises(ValueError):
            record_mod.validate_run_record(bad)
        bad = make_record(FLAWED[0], "treatment")
        bad["review"]["dispositions"] = [disposition("F1", defect_id="D1")]
        with self.assertRaises(ValueError):
            record_mod.validate_run_record(bad)
        bad = make_record(FLAWED[0], "treatment")
        bad["timing_ms"]["review"] = -1
        with self.assertRaises(ValueError):
            record_mod.validate_run_record(bad)
        bad = make_record(FLAWED[0], "treatment")
        bad["arm"] = "meta"
        with self.assertRaises(ValueError):
            record_mod.validate_run_record(bad)

    def test_boolean_values_are_not_accepted_as_integers(self):
        invalid_fields = (
            (("schema_version",), True),
            (("pair_index",), True),
            (("order_in_pair",), True),
            (("implementation", "repair_cycles"), True),
            (("implementation", "verification_cost"), True),
            (("timing_ms", "review"), True),
            (("usage", "token_cost"), True),
            (("usage", "context_bytes"), True),
            (("usage", "currency_cost"), True),
        )
        for path, value in invalid_fields:
            with self.subTest(field=path):
                record = make_record(FLAWED[0], "treatment")
                target = record
                for key in path[:-1]:
                    target = target[key]
                target[path[-1]] = value
                with self.assertRaises(ValueError):
                    record_mod.validate_run_record(record)



class AdjudicationTests(unittest.TestCase):
    def test_confirmed_missed_false_positive_duplicate(self):
        data = load_corpus(CORPUS_DIR)
        oracle = data["oracles"][FLAWED[0]]
        record = make_record(
            FLAWED[0], "treatment",
            findings=[finding("F1"), finding("F2"), finding("F3")],
            dispositions=[disposition("F1", defect_id="D1"),
                          disposition("F2", defect_id="D2"),
                          disposition("F3", status="rejected")],
        )
        result = adjudicate_run(record, oracle)
        self.assertEqual(result["confirmed_defects"], ["D1", "D2"])
        self.assertEqual(result["missed_defects"], [])
        self.assertEqual(result["false_positives"], 1)
        self.assertEqual(result["duplicate_findings"], 0)
        duplicate = make_record(
            FLAWED[0], "treatment",
            findings=[finding("F1"), finding("F4")],
            dispositions=[disposition("F1", defect_id="D1"),
                          disposition("F4", status="confirmed",
                                      defect_id="D1", duplicate_of="F1")],
        )
        result = adjudicate_run(duplicate, oracle)
        self.assertEqual(result["duplicate_findings"], 1)
        self.assertEqual(result["confirmed_defects"], ["D1"])
        self.assertEqual(result["missed_defects"], ["D2"])

    def test_confirmed_unknown_defect_fails_closed(self):
        data = load_corpus(CORPUS_DIR)
        oracle = data["oracles"][FLAWED[0]]
        record = make_record(
            FLAWED[0], "treatment",
            findings=[finding("F9")],
            dispositions=[disposition("F9", defect_id="D9")],
        )
        with self.assertRaises(ValueError):
            adjudicate_run(record, oracle)


class VerdictTests(unittest.TestCase):
    def _corpus(self):
        return load_corpus(CORPUS_DIR)


    def test_sample_requires_all_repeated_runs_per_arm(self):
        protocol = pinned_protocol()
        protocol["sample"]["repeated_runs_per_design_per_arm"] = 2
        report = aggregate(six_design_records(), self._corpus(), protocol)
        self.assertEqual(report["complete_design_count"], 0)
        self.assertEqual(report["verdict"], "INCONCLUSIVE")

    def test_pinned_protocol_requires_repeated_run_count(self):
        protocol = pinned_protocol()
        protocol["sample"]["repeated_runs_per_design_per_arm"] = None
        report = aggregate(six_design_records(), self._corpus(), protocol)
        self.assertEqual(report["verdict"], "INCONCLUSIVE")
        self.assertTrue(any("not pinned" in reason
                            for reason in report["decision_reasons"]))

    def test_go_path(self):
        records = six_design_records()
        report = aggregate(records, self._corpus(), pinned_protocol())
        self.assertEqual(report["verdict"], "GO")
        self.assertEqual(report["complete_design_count"], 6)
        advantage = sum(
            1 for d in report["designs"]
            if d["complete"] and d["treatment_new_beyond"]
        )
        self.assertEqual(advantage, 3)

    def test_no_go_effect_below_bar(self):
        records = six_design_records(treatment_caught=False)
        report = aggregate(records, self._corpus(), pinned_protocol())
        self.assertEqual(report["verdict"], "NO_GO")
        self.assertTrue(any("minimum worthwhile improvement" in r
                            for r in report["decision_reasons"]))

    def test_no_go_false_positive_blowout(self):
        records = six_design_records()
        for record in records:
            if record["arm"] == "treatment" and record["case_id"] in FLAWED:
                for i in range(3):
                    fid = "FP-%d" % i
                    record["review"]["findings"].append(finding(fid))
                    record["review"]["dispositions"].append(
                        disposition(fid, status="rejected"))
        report = aggregate(records, self._corpus(), pinned_protocol())
        self.assertEqual(report["verdict"], "NO_GO")
        self.assertTrue(any("false-positive tolerance" in r
                            for r in report["decision_reasons"]))

    def test_inconclusive_insufficient_sample(self):
        records = six_design_records()
        records = [r for r in records if r["case_id"] not in SOUND[:2]]
        report = aggregate(records, self._corpus(), pinned_protocol())
        self.assertEqual(report["verdict"], "INCONCLUSIVE")
        self.assertTrue(any("insufficient sample" in r
                            for r in report["decision_reasons"]))

    def test_inconclusive_straddling_effect(self):
        records = six_design_records(treatment_caught=False)
        treatment_flawed = [
            r for r in records
            if r["arm"] == "treatment" and r["case_id"] in FLAWED
        ]
        for idx, record in enumerate(treatment_flawed):
            if idx < 2:
                record["review"]["findings"] = [finding("D1")]
                record["review"]["dispositions"] = [
                    disposition("D1", defect_id="D1")]
            else:
                record["review"]["findings"] = []
                record["review"]["dispositions"] = []
        report = aggregate(records, self._corpus(), pinned_protocol())
        self.assertEqual(report["verdict"], "INCONCLUSIVE")
        self.assertTrue(any("straddles" in r
                            for r in report["decision_reasons"]))

    def test_inconclusive_unpinned_thresholds(self):
        records = six_design_records()
        report = aggregate(records, self._corpus(),
                           load_protocol(PROTOCOL_FILE))
        self.assertEqual(report["verdict"], "INCONCLUSIVE")
        self.assertTrue(any("not pinned" in r
                            for r in report["decision_reasons"]))

    def test_inconclusive_isolation_missing(self):
        records = six_design_records(isolation="unverified")
        report = aggregate(records, self._corpus(), pinned_protocol())
        self.assertEqual(report["verdict"], "INCONCLUSIVE")
        self.assertTrue(any("isolation" in r
                            for r in report["decision_reasons"]))

    def test_single_impressive_finding_is_never_the_criterion(self):
        records = six_design_records(treatment_caught=False)
        treatment_flawed = [
            r for r in records
            if r["arm"] == "treatment" and r["case_id"] in FLAWED
        ]
        record = treatment_flawed[0]
        record["review"]["findings"] = [finding("D1"), finding("D2")]
        record["review"]["dispositions"] = [
            disposition("D1", defect_id="D1"),
            disposition("D2", defect_id="D2"),
        ]
        report = aggregate(records, self._corpus(), pinned_protocol())
        self.assertEqual(report["verdict"], "INCONCLUSIVE")
        self.assertNotEqual(report["verdict"], "GO")

    def test_validate_protocol_rejects_unknown_thresholds(self):
        protocol = pinned_protocol()
        del protocol["thresholds"]["mwi_advantage_rate"]
        with self.assertRaises(ValueError):
            validate_protocol(protocol)
        protocol = pinned_protocol()
        protocol["sample"]["min_complete_designs"] = "many"
        with self.assertRaises(ValueError):
            validate_protocol(protocol)

    def test_wilson_interval_matches_458_formula(self):
        # 3/6 in a 95% Wilson interval spans 0.5 -> straddle semantics.
        low, high = wilson_interval(3, 6)
        self.assertLess(low, 0.5)
        self.assertGreater(high, 0.5)
        low, high = wilson_interval(0, 6)
        self.assertLess(high, 0.5)


if __name__ == "__main__":
    unittest.main()
