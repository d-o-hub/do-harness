"""Aggregation and decision-rule verdict for #499 recorded runs.

The distinct design (case) is the sampling unit; repeated runs of one design
are measurement repeats, not independent design cases. The verdict is
GO / NO_GO / INCONCLUSIVE computed from predeclared thresholds only, a single
impressive finding is never a criterion, and an unpinned threshold set (the
committed placeholder state) always yields INCONCLUSIVE.
"""

import json
import math
from pathlib import Path

from .adjudicate import adjudicate_records
from .corpus import EXPERIMENT
from .record import COMPLETED_OUTCOMES, validate_run_record

THRESHOLD_KEYS = (
    "mwi_advantage_rate",
    "false_positive_tolerance_per_design",
    "regression_tolerance_rate",
    "max_review_time_minutes",
    "max_usage_cost_usd",
)
VERDICTS = ("GO", "NO_GO", "INCONCLUSIVE")


def wilson_interval(successes, total):
    """Wilson 95% binomial interval (same formula as #458 runner)."""
    z = 1.96
    ratio = successes / total
    scale = 1 + z * z / total
    center = (ratio + z * z / (2 * total)) / scale
    radius = z * math.sqrt(
        ratio * (1 - ratio) / total + z * z / (4 * total * total)
    ) / scale
    return [round(center - radius, 6), round(center + radius, 6)]




def pinned_thresholds(protocol):
    thresholds = protocol["thresholds"]
    if protocol.get("status") != "pinned":
        return None
    if any(thresholds.get(key) is None for key in THRESHOLD_KEYS):
        return None
    if protocol["sample"]["min_complete_designs"] is None:
        return None
    return thresholds


def _median(values):
    if not values:
        return None
    ordered = sorted(values)
    mid = len(ordered) // 2
    if len(ordered) % 2:
        return ordered[mid]
    return (ordered[mid - 1] + ordered[mid]) / 2


def validate_protocol(protocol):
    if protocol.get("protocol_version") != 1:
        raise ValueError("invalid protocol_version")
    if protocol.get("experiment") != EXPERIMENT:
        raise ValueError("invalid protocol experiment")
    thresholds = protocol["thresholds"]
    for key in THRESHOLD_KEYS:
        if key not in thresholds:
            raise ValueError("missing threshold %r" % key)
    sample = protocol["sample"]
    for key in ("repeated_runs_per_design_per_arm", "min_complete_designs"):
        if key not in sample:
            raise ValueError("missing sample parameter %r" % key)
    for key in ("repeated_runs_per_design_per_arm", "min_complete_designs"):
        value = sample[key]
        if value is not None and (type(value) is not int or value <= 0):
            raise ValueError("invalid sample parameter %r" % key)
    return protocol


def load_protocol(path):
    return validate_protocol(
        json.loads(Path(path).read_text(encoding="utf-8"))
    )
def summarize_designs(adjudicated, records, corpus):
    """Summarize per-design completeness and treatment measurements."""
    record_map = {}
    for record in records:
        record_map.setdefault(record["case_id"], []).append(record)

    designs = []
    for case in corpus["cases"]:
        case_id = case["id"]
        case_records = record_map.get(case_id, [])
        baseline = [r for r in case_records if r["arm"] == "baseline"]
        treatment = [r for r in case_records if r["arm"] == "treatment"]
        baseline_completed = [r for r in baseline
                              if r["review"]["outcome"] in COMPLETED_OUTCOMES]
        treatment_completed = [r for r in treatment
                               if r["review"]["outcome"] in COMPLETED_OUTCOMES]
        complete = bool(baseline_completed and treatment_completed)
        unit = {
            "case_id": case_id,
            "class": case["class"],
            "variant": case["variant"],
            "complete": complete,
            "baseline_runs": len(baseline),
            "treatment_runs": len(treatment),
            "baseline_completed_runs": len(baseline_completed),
            "treatment_completed_runs": len(treatment_completed),
        }
        if complete:
            runs = adjudicated[case_id]["runs"]
            treatment_runs = [
                r for r in runs
                if r["arm"] == "treatment" and "confirmed_defects" in r
            ]
            baseline_runs = [
                r for r in runs
                if r["arm"] == "baseline" and "confirmed_defects" in r
            ]
            unit["treatment_new_beyond"] = sorted({
                d for r in treatment_runs for d in r.get("new_beyond_baseline", [])
            })
            unit["baseline_confirmed"] = sorted({
                d for r in baseline_runs for d in r["confirmed_defects"]
            })
            unit["treatment_false_positives"] = sum(
                r.get("false_positives", 0) for r in treatment_runs
            )
            unit["treatment_unresolved"] = sum(
                r.get("unresolved_findings", 0) for r in treatment_runs
            )
            unit["treatment_duplicates"] = sum(
                r.get("duplicate_findings", 0) for r in treatment_runs
            )
            impl_treatment = [r for r in treatment if r["implementation"]["ran"]]
            impl_baseline = [r for r in baseline if r["implementation"]["ran"]]
            unit["implementation_runs"] = bool(impl_treatment and impl_baseline)
            unit["treatment_regression_seen"] = any(
                r["implementation"]["regression_seen"] for r in impl_treatment
            )
            review_ms = [r["timing_ms"]["review"] for r in treatment]
            unit["treatment_review_ms_recorded"] = all(
                r["timing_ms"]["review"] is not None for r in treatment
            )
            unit["treatment_review_median_min"] = (
                round(_median(review_ms) / 60000.0, 3) if review_ms else None
            )
            usage_ok = all(
                r["usage"]["coverage"] in ("full", "partial")
                and r["usage"]["provenance"] != "unavailable"
                for r in treatment
            )
            unit["treatment_usage_recorded"] = usage_ok
            unit["treatment_usage_cost_usd"] = (
                round(sum(r["usage"].get("currency_cost") or 0 for r in treatment), 4)
                if usage_ok else None
            )
            unit["treatment_isolation_verified"] = all(
                r["identities"]["isolation_evidence"] == "verified"
                for r in treatment
            )
        designs.append(unit)
    return designs




def decide(designs, protocol):
    """Compute the verdict from complete designs and pinned thresholds."""
    thresholds = pinned_thresholds(protocol)
    if thresholds is None:
        return {
            "verdict": "INCONCLUSIVE",
            "reasons": ["thresholds not pinned from #498 pilot feasibility data"],
        }
    n_required = protocol["sample"]["min_complete_designs"]
    complete = [d for d in designs if d["complete"]]
    n = len(complete)
    if n < n_required:
        return {
            "verdict": "INCONCLUSIVE",
            "reasons": ["insufficient sample: %d/%d complete designs" % (n, n_required)],
        }

    for unit in complete:
        if not unit["treatment_isolation_verified"]:
            return {
                "verdict": "INCONCLUSIVE",
                "reasons": ["isolation evidence missing for case %s" % unit["case_id"]],
            }
        if not unit["treatment_review_ms_recorded"]:
            return {
                "verdict": "INCONCLUSIVE",
                "reasons": ["review timing not recorded for case %s" % unit["case_id"]],
            }
        if not unit["treatment_usage_recorded"]:
            return {
                "verdict": "INCONCLUSIVE",
                "reasons": ["usage cost not recorded for case %s" % unit["case_id"]],
            }

    fp_tolerance = thresholds["false_positive_tolerance_per_design"] * n
    fp_total = sum(d["treatment_false_positives"] for d in complete)
    if fp_total > fp_tolerance:
        return {
            "verdict": "NO_GO",
            "reasons": ["false-positive tolerance exceeded: %d > %d" % (fp_total, fp_tolerance)],
        }

    regression_units = [d for d in complete if d["implementation_runs"]]
    if regression_units:
        regression_rate = sum(d["treatment_regression_seen"] for d in regression_units) / len(regression_units)
        if regression_rate > thresholds["regression_tolerance_rate"]:
            return {
                "verdict": "NO_GO",
                "reasons": ["regression tolerance exceeded: %.3f > %.3f"
                            % (regression_rate, thresholds["regression_tolerance_rate"])],
            }

    review_median = _median([d["treatment_review_median_min"] for d in complete])
    if review_median is not None and review_median > thresholds["max_review_time_minutes"]:
        return {
            "verdict": "NO_GO",
            "reasons": ["review time exceeds cap: %.3f min > %.3f min"
                        % (review_median, thresholds["max_review_time_minutes"])],
        }

    usage_total = sum(d["treatment_usage_cost_usd"] or 0 for d in complete)
    if usage_total > thresholds["max_usage_cost_usd"]:
        return {
            "verdict": "NO_GO",
            "reasons": ["usage cost exceeds cap: %.4f USD > %.4f USD"
                        % (usage_total, thresholds["max_usage_cost_usd"])],
        }

    mwi = thresholds["mwi_advantage_rate"]
    advantage = sum(1 for d in complete if d["treatment_new_beyond"])
    rate = advantage / n
    low, high = wilson_interval(advantage, n)
    if rate >= mwi and low > 0:
        return {
            "verdict": "GO",
            "reasons": ["advantage rate %.3f >= mwi %.3f and lower bound %.3f > 0"
                        % (rate, mwi, low)],
        }
    if high < mwi:
        return {
            "verdict": "NO_GO",
            "reasons": ["effect below minimum worthwhile improvement: upper bound %.3f < %.3f"
                        % (high, mwi)],
        }
    return {
        "verdict": "INCONCLUSIVE",
        "reasons": ["effect estimate straddles threshold: rate %.3f, interval [%.3f, %.3f]"
                    % (rate, low, high)],
    }


def aggregate(records, corpus, protocol):
    """Full pipeline: validate, adjudicate, summarize, decide."""
    validated = [validate_run_record(record) for record in records]
    adjudicated = adjudicate_records(validated, corpus)
    designs = summarize_designs(adjudicated, validated, corpus)
    verdict = decide(designs, protocol)
    return {
        "schema_version": 1,
        "experiment": EXPERIMENT,
        "protocol_version": protocol["protocol_version"],
        "protocol_status": protocol.get("status"),
        "verdict": verdict["verdict"],
        "decision_reasons": verdict["reasons"],
        "designs": designs,
        "complete_design_count": sum(1 for d in designs if d["complete"]),
    }
