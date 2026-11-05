"""Adopter-corpus support: validation, usage normalization, budgets, analysis.

Keeps protocol-2 usage normalization, per-arm identity provenance, pilot-based
wall budgets, and the preregistered contrast analysis out of the runner core.
"""
import math

# Provenance vocabularies (fixed, documented strings).
IDENTITY_PROVENANCES = ("operator-supplied", "adapter-reported", "provider-observed", "unavailable")
USAGE_PROVENANCES = ("adapter-reported", "provider-observed", "estimated", "unavailable")

# The eight fixed scenario classes for the adopter corpus.
SCENARIOS = frozenset({
    "multi-file-filter-fix",
    "source-edit-after-green",
    "policy-edit-invalidates-evidence",
    "identical-failures-cap",
    "alternating-failures-cap",
    "fresh-process-resume-no-duplicate",
    "denied-outside-write",
    "unsupported-capability",
})

# Value limits: tokens/bytes are non-negative integers, currency is a finite
# non-negative float with an explicit unit; booleans and non-finite values are
# rejected. "Over-budget" values are rejected outright.
USAGE_KINDS = {"tokens": int, "bytes": int, "interventions": int}
USAGE_UNITS = {"tokens": "tokens", "bytes": "bytes", "interventions": "interventions"}
CURRENCY_METRIC = "currency"
USAGE_LIMITS = {"tokens": 10 ** 12, "bytes": 10 ** 12, "interventions": 10 ** 6, CURRENCY_METRIC: 10 ** 6}
USAGE_METRICS = (*USAGE_KINDS, CURRENCY_METRIC)

# Canned patch action payloads referenced by the corpus.
POLICY_STRICT_SIGNAL = """\
language = "node"

[[sensors]]
name = "build"
argv = ["node", "--check", "cli.js", "parser.js", "filter.js", "summary.js"]
when-changed = ["**/*.js", "package.json"]

[[sensors]]
    return {
        "value": None,
        "unit": USAGE_UNITS.get(metric),
        "provenance": "unavailable",
        "reason": reason,
        "observed": None,
        "expected": None,
        "partial_subtotal": None,
        "complete_total": None,
        "status": "unavailable",
    }
when-changed = ["strict-signal.txt"]
"""

GOLDEN_RESULT_JSON = (
    '{"total":3,"by_severity":{"ERROR":1,"INFO":1,"WARN":1},'
    '"records":[{"severity":"INFO","component":"db","message":"retry scheduled"},'
    '{"severity":"WARN","component":"db","message":"slow query 120ms"},'
    '{"severity":"ERROR","component":"db","message":"connection refused"}]}\n'
)

# Preregistered comparison design. These are thresholds and stopping rules,
# never predictions.
DESIGN = {
    "paired_repeats": 20,
    "counterbalanced_order": True,
    "identical_treatment_control": True,
    "one_feature_contrasts": [{"id": "feature-change", "variable": "single declared feature"}],
    "no_post_hoc_exclusions": True,
    "confidence": 0.95,
    "stopping": "fixed paired repeats; early budget exhaustion is INCONCLUSIVE",
    "thresholds": {
        "material_cost_improvement": 0.10,
        "material_intervention_improvement_points": 10,
        "verified_success_worse_allowed_points": 5,
        "zero_false_success": True,
        "zero_missed_planted_regressions": True,
    },
}


def unavailable_metric(metric, reason):
    return {
        "value": None,
        "unit": USAGE_UNITS.get(metric),
        "provenance": "unavailable",
        "reason": reason,
        "observed": None,
        "expected": None,
        "partial_subtotal": None,
        "complete_total": None,
        "status": "unavailable",
    }


def _bounded_label(value):
    """Opaque observed/expected sample labels: bounded strings only."""
    if not isinstance(value, str) or not 1 <= len(value) <= 100:
        return None
    return value


def normalize_metric(metric, spec):
    """Validate one usage metric from an adapter response.

    Returns a validated metric object, or a reasoned unavailable object when
    the value is missing, a boolean/non-finite number, negative, over budget,
    unitless for currency, or carries unknown provenance.
    """
    if not isinstance(spec, dict):
        return unavailable_metric(metric, "malformed usage entry")
    if "value" not in spec:
        return unavailable_metric(metric, "usage value missing")
    value = spec["value"]
    if isinstance(value, bool) or not isinstance(value, (int, float)):
        return unavailable_metric(metric, "usage value must be numeric, not boolean/string")
    if isinstance(value, float) and not math.isfinite(value):
        return unavailable_metric(metric, "usage value must be finite")
    limit = USAGE_LIMITS[metric]
    if value < 0 or value > limit:
        return unavailable_metric(metric, "usage value out of bounds")
    if metric in USAGE_KINDS:
        if not isinstance(value, int):
            return unavailable_metric(metric, f"{metric} must be a non-negative integer")
    else:
        if not isinstance(value, (int, float)):
            return unavailable_metric(metric, "currency must be a finite non-negative number")
    unit = spec.get("unit")
    if metric == CURRENCY_METRIC:
        if not isinstance(unit, str) or not 1 <= len(unit) <= 24:
            return unavailable_metric(metric, "currency requires an explicit unit")
    else:
        unit = unit if isinstance(unit, str) and unit else USAGE_UNITS[metric]
    provenance = spec.get("provenance", "adapter-reported")
    if provenance not in USAGE_PROVENANCES:
        return unavailable_metric(metric, "unknown usage provenance")
    complete_total = spec.get("complete_total")
    if complete_total is not None:
        if isinstance(complete_total, bool) or not isinstance(complete_total, (int, float)) \
                or not math.isfinite(float(complete_total)) or float(complete_total) < 0:
            complete_total = None
        elif isinstance(complete_total, float):
            complete_total = int(complete_total)
    return {
        "value": value,
        "unit": unit,
        "provenance": provenance,
        "reason": None,
        "observed": _bounded_label(spec.get("observed")),
        "expected": _bounded_label(spec.get("expected")),
        "partial_subtotal": None,
        "complete_total": complete_total,
    }


def normalize_usage(raw):
    """Normalize a protocol-2 `usage` object into metric objects."""
    if not isinstance(raw, dict):
        return {}
    metrics = {}
    for metric in USAGE_METRICS:
        if metric in raw:
            metrics[metric] = normalize_metric(metric, raw[metric])
    return metrics


def legacy_flat_usage(adapter_res):
    """Deprecated protocol-1 flat usage fields mapped to adapter-reported metrics.

    Kept only for backward compatibility with older trusted adapters; the
    documented protocol is v2 and any claim here is adapter-reported, never
    provider-observed.
    """
    metrics = {}
    flat = {
        "tokens": ("token_cost", USAGE_KINDS["tokens"], USAGE_UNITS["tokens"]),
        "currency": ("currency_cost", None, None),
        "bytes": ("context_bytes", USAGE_KINDS["bytes"], USAGE_UNITS["bytes"]),
    }
    for metric, (key, kind, unit) in flat.items():
        value = adapter_res.get(key)
        if isinstance(value, bool) or not isinstance(value, (int, float)):
            continue
        if isinstance(value, float) and not math.isfinite(value):
            continue
        if value < 0 or value > USAGE_LIMITS[metric]:
            continue
        if kind is not None and not isinstance(value, int):
            continue
        if metric == "currency" and not isinstance(value, (int, float)):
            continue
        if metric == "currency" and unit is None:
            unit = "usd"
        metrics[metric] = {
            "value": value,
            "unit": unit,
            "provenance": "adapter-reported",
            "reason": None,
            "observed": None,
            "expected": None,
            "partial_subtotal": None,
            "complete_total": None,
        }
    return metrics


def merge_usage(current, fresh):
    """Merge per-attempt usage into a per-case running metric object."""
    merged = dict(current)
    for metric, entry in fresh.items():
        if metric not in merged:
            merged[metric] = {
                "value": None,
                "unit": entry["unit"],
                "provenance": entry["provenance"],
                "reason": None,
                "observed": [],
                "expected": [],
                "partial_subtotal": None,
                "complete_total": None,
            }
        target = merged[metric]
        if entry["value"] is None:
            continue
        target["value"] = (target["value"] or 0) + entry["value"]
        if entry["observed"] is not None:
            target["observed"].append(entry["observed"])
        if entry["expected"] is not None:
            target["expected"].append(entry["expected"])
        if entry["complete_total"] is not None:
            target["complete_total"] = entry["complete_total"]
    return merged


def summarize_usage(case_results):
    """Arm-level usage aggregations: labelled partial subtotal, null complete
    total unless every case reported the metric without an unavailable reason."""
    summary = {}
    for metric in USAGE_METRICS:
        values = []
        complete = True
        for row in case_results:
            entry = row.get("usage", {}).get(metric)
            if entry is None or entry.get("value") is None:
                complete = False
                continue
            values.append(entry["value"])
        if not values:
            summary[metric] = unavailable_metric(metric, "no adapter usage reported")
            continue
        partial = sum(values)
        summary[metric] = {
            "unit": USAGE_UNITS.get(metric),
            "provenance": "adapter-reported",
            "partial_subtotal": partial,
            "complete_total": partial if complete else None,
            "status": "measured" if complete else "partial",
            "reason": None,
        }
    return summary


def base_identities(harness_revision):
    identities = {}
    for key in ("client", "adapter", "harness", "model"):
        identities[key] = {"value": None, "provenance": "unavailable", "digest": None}
    identities["harness"] = {
        "value": harness_revision,
        "provenance": "operator-supplied",
        "digest": None,
    }
    return identities


def parse_identity_flags(flags):
    """Parse repeated `KEY=VALUE[:PROVENANCE]` operator flags."""
    identities = {}
    for flag in flags or ():
        if "=" not in flag:
            raise ValueError(f"identity flag must be KEY=VALUE[:PROVENANCE]: {flag}")
        key, _, rest = flag.partition("=")
        if key not in ("client", "adapter", "harness", "model"):
            raise ValueError(f"unknown identity key: {key}")
        value, _, provenance = rest.partition(":")
        if not 1 <= len(value) <= 200:
            raise ValueError(f"identity value out of bounds: {key}")
        if provenance and provenance not in IDENTITY_PROVENANCES:
            raise ValueError(f"unknown identity provenance: {provenance}")
        identities[key] = {
            "value": value,
            "provenance": provenance or "operator-supplied",
            "digest": None,
        }
    return identities


def merge_identities(base, adapter_res):
    """Overlay adapter-reported identities, honoring provenance rules."""
    merged = {key: dict(entry) for key, entry in base.items()}
    raw = adapter_res.get("identities") if isinstance(adapter_res, dict) else None
    if not isinstance(raw, dict):
        return merged
    for key, spec in raw.items():
        if key not in merged or not isinstance(spec, dict):
            continue
        value = spec.get("value")
        if not isinstance(value, str) or not 1 <= len(value) <= 200:
            continue
        provenance = spec.get("provenance", "adapter-reported")
        if provenance not in IDENTITY_PROVENANCES:
            continue
        digest = spec.get("digest")
        if digest is not None and (not isinstance(digest, str) or not 1 <= len(digest) <= 100):
            digest = None
        merged[key] = {"value": value, "provenance": provenance, "digest": digest}
    return merged


def pilot_wall_seconds(profile, caps):
    caps = caps or {}
    key = "adopter_seconds" if profile == "adopter" else "standard_seconds"
    return caps.get(key, 120 if profile != "adopter" else 600)


def wall_budget(profile, caps, pilot_max_seconds):
    """Wall budget from a recorded pilot: ceil(2 * max end-to-end), clamped by
    the profile cap; exceeding the adopter cap blocks instead of raising."""
    if pilot_max_seconds is None or pilot_max_seconds < 0:
        raise ValueError("pilot measured no end-to-end cases")
    computed = max(1, math.ceil(2 * pilot_max_seconds))
    caps = caps or {}
    standard_cap = caps.get("standard_seconds", 120)
    adopter_cap = caps.get("adopter_seconds", 600)
    cap = adopter_cap if profile == "adopter" else standard_cap
    if computed > cap:
        if profile == "adopter":
            raise ValueError(
                f"pilot wall budget {computed}s exceeds the adopter cap {adopter_cap}s; "
                f"block instead of silently raising it"
            )
        raise ValueError(
            f"pilot wall budget {computed}s exceeds the standard cap {standard_cap}s; "
            f"rerun with --profile adopter (cap {adopter_cap}s)"
        )
    return computed


def paired_interval(control, treatment, confidence=0.95):
    """Two-sided paired interval on differences (treatment - control).

    Falls back to a descriptive median gap when fewer than two pairs exist.
    """
    if len(control) != len(treatment) or not control:
        return None
    diffs = [t - c for c, t in zip(control, treatment)]
    count = len(diffs)
    mean = sum(diffs) / count
    if count < 2:
        return {"n": count, "mean_diff": round(mean, 6), "ci": None}
    variance = sum((d - mean) ** 2 for d in diffs) / (count - 1)
    std_error = math.sqrt(variance / count)
    # Two-sided t quantile approximation for p = (1 - confidence) / 2.
    z = 1.96
    if count < 30:
        df = count - 1
        # t quantile approximation (Abramowitz & Stegun 26.7.5 style).
        t = z + (z ** 3 + z) / (4 * df) + (5 * z ** 5 + 16 * z ** 3 + 3 * z) / (96 * df ** 2)
    else:
        t = z
    half = t * std_error
    return {
        "n": count,
        "mean_diff": round(mean, 6),
        "ci": [round(mean - half, 6), round(mean + half, 6)],
    }


def contrast(control, treatment, design=None):
    """Preregistered contrast verdict from paired metric arrays.

    `control`/`treatment` are dicts mapping metric names to paired value lists
    (e.g. `cost_per_verified_success`, `interventions`, `verified_success`).
    Returns a structured decision; missing data yields INCONCLUSIVE.
    """
    design = design or DESIGN
    thresholds = design["thresholds"]
    metrics = {}
    decision = "inconclusive"
    rationale = []
    for metric, control_values in control.items():
        if not isinstance(control_values, list):
            continue
        treatment_values = treatment.get(metric, [])
        interval = paired_interval(control_values, treatment_values)
        metrics[metric] = interval
    false_success = control.get("false_success", 0) + treatment.get("false_success", 0)
    missed = control.get("missed_planted_regressions", 0) + treatment.get("missed_planted_regressions", 0)
    if missed or false_success:
        rationale.append(f"false successes {false_success}, missed planted regressions {missed}")

    cost = metrics.get("cost_per_verified_success")
    interventions = metrics.get("interventions")
    success = metrics.get("verified_success")
    insufficient = any(entry is not None and entry.get("ci") is None
                       for entry in (cost, interventions, success))
    if insufficient:
        rationale.append("inconclusive: fewer than two paired repeats per metric")
    cost_benefit = False
    intervention_benefit = False
    if cost is not None and cost["ci"] is not None:
        control_mean = sum(control["cost_per_verified_success"]) / len(control["cost_per_verified_success"])
        # Material benefit: >=10% lower cost per verified success by the upper
        # CI bound.
        cost_benefit = cost["ci"][1] <= -thresholds["material_cost_improvement"] * control_mean
    if interventions is not None and interventions["ci"] is not None:
        # Fewer interventions: (control - treatment) >= 10 points by the CI.
        # The stored interval is on treatment - control, so the gap CI is the
        # mirrored interval.
        gap_ci_low = -interventions["ci"][1]
        intervention_benefit = gap_ci_low >= thresholds["material_intervention_improvement_points"]
    success_ok = True
    if success is not None and success["ci"] is not None:
        # Verified-success rate no more than 5 points worse than control.
        success_ok = success["ci"][1] >= -thresholds["verified_success_worse_allowed_points"]
    if insufficient:
        rationale.append("inconclusive: paired sample too small for a verdict")
    elif (cost_benefit or intervention_benefit) and success_ok and not missed and not false_success:
        decision = "material-benefit"
        rationale.append("declared benefit threshold met within the 95% paired interval")
    elif not missed and not false_success and not (cost_benefit or intervention_benefit):
        decision = "no-material-benefit"
        rationale.append("neither cost nor intervention threshold met")
    else:
        rationale.append("inconclusive: thresholds unmet or evidence incomplete")
    return {
        "decision": decision,
        "rationale": rationale,
        "metrics": metrics,
        "false_success": false_success,
        "missed_planted_regressions": missed,
        "thresholds": thresholds,
    }
