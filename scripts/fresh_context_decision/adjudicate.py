"""Deterministic adjudication of run findings against the frozen oracle.

Adjudication is oracle-anchored: a finding is confirmed only when the
adjudicator disposition maps it to a defect id that exists in the frozen
ground truth. A confirmed mapping to an unknown defect is a data error, not a
silent drop, so the machinery fails closed. Findings without a disposition, or
disposed unresolved, stay unresolved and make the run incomplete for the
primary outcome.
"""

from .record import COMPLETED_OUTCOMES
from .corpus import load_oracle


def adjudicate_run(record, oracle):
    """Return per-run adjudication counts for one completed review."""
    review = record["review"]
    findings = review.get("findings", [])
    dispositions = review.get("dispositions", [])
    defect_ids = [d["id"] for d in oracle["defects"]]
    expected = set(oracle["predeclared_expected_findings"])
    by_finding = {}
    for disposition in dispositions:
        if disposition["id"] in by_finding:
            raise ValueError("duplicate disposition for finding %r"
                             % disposition["id"])
        by_finding[disposition["id"]] = disposition

    confirmed = set()
    false_positives = 0
    duplicates = 0
    unresolved = 0
    for finding in findings:
        finding_id = finding["id"]
        disposition = by_finding.get(finding_id)
        if disposition is None or disposition["status"] == "unresolved":
            unresolved += 1
            continue
        if disposition.get("duplicate_of"):
            duplicates += 1
            continue
        if disposition["status"] == "rejected":
            false_positives += 1
            continue
        # confirmed
        defect_id = disposition["defect_id"]
        if defect_id not in defect_ids:
            raise ValueError(
                "finding %r confirmed to unknown defect %r (oracle frozen)"
                % (finding_id, defect_id)
            )
        confirmed.add(defect_id)

    missed = sorted(expected - confirmed)
    confirmed_sorted = sorted(confirmed)
    result = {
        "confirmed_defects": confirmed_sorted,
        "missed_defects": missed,
        "false_positives": false_positives,
        "duplicate_findings": duplicates,
        "unresolved_findings": unresolved,
        "new_beyond_baseline": [],
    }
    return result


def pair_with_baseline(record, baseline_records, oracle):
    """Attach baseline-overlap fields using the paired baseline arm runs."""
    result = adjudicate_run(record, oracle)
    baseline_confirmed = set()
    for baseline in baseline_records:
        if baseline["review"]["outcome"] not in COMPLETED_OUTCOMES:
            continue
        baseline_confirmed |= set(adjudicate_run(baseline, oracle)["confirmed_defects"])
    result["baseline_overlap"] = sorted(set(result["confirmed_defects"]) & baseline_confirmed)
    result["new_beyond_baseline"] = sorted(
        set(result["confirmed_defects"]) - baseline_confirmed
    )
    return result


def adjudicate_records(records, corpus):
    """Adjudicate every completed run in a record set.

    Returns {case_id: {"runs": [per-run adjudication with arm/run_id],
                        "oracle": oracle}} for cases present in the corpus.
    """
    oracle_by_case = corpus["oracles"]
    output = {}
    for record in records:
        case_id = record["case_id"]
        if case_id not in oracle_by_case:
            raise ValueError("record references unknown case %r" % case_id)
        oracle = oracle_by_case[case_id]
        pair = output.setdefault(case_id, {"runs": [], "oracle": oracle})
        if record["review"]["outcome"] in COMPLETED_OUTCOMES:
            baseline_records = [
                r for r in records
                if r["case_id"] == case_id and r["arm"] == "baseline"
                and r["pair_index"] == record["pair_index"]
            ]
            adj = pair_with_baseline(record, baseline_records, oracle)
        else:
            adj = {"incomplete_outcome": record["review"]["outcome"],
                   "reason": record["review"].get("reason")}
        adj["arm"] = record["arm"]
        adj["run_id"] = record["run_id"]
        adj["pair_index"] = record["pair_index"]
        pair["runs"].append(adj)
    return output
