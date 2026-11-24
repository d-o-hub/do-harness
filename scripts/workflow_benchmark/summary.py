"""Aggregation helpers for workflow benchmark rows (kept under the LOC cap)."""
import math


def percentile(values, p):
    if not values:
        return None
    sorted_vals = sorted(values)
    rank = math.ceil(p * len(sorted_vals))
    index = max(0, min(rank - 1, len(sorted_vals) - 1))
    return sorted_vals[index]


def interval(successes, total):
    """Wilson 95% binomial interval: descriptive, not a paired causal estimate."""
    z = 1.96
    ratio = successes / total
    scale = 1 + z * z / total
    center = (ratio + z * z / (2 * total)) / scale
    radius = z * math.sqrt(ratio * (1 - ratio) / total + z * z / (4 * total * total)) / scale
    return [round(center - radius, 6), round(center + radius, 6)]


def summarize(rows):
    count = len(rows)
    success = sum(row["final_success"] for row in rows)
    first_passes = sum(r["first_attempt_pass"] for r in rows)
    repair_cycles = sum(r["repair_cycles"] for r in rows)
    attempts = sum(r["attempts"] for r in rows)
    verification_cost = sum(r["verification_cost"] for r in rows)

    elapsed_vals = [r["elapsed_ms"] for r in rows if r["elapsed_ms"] is not None]
    elapsed_summary = {
        "median": percentile(elapsed_vals, 0.50),
        "p95": percentile(elapsed_vals, 0.95),
    } if elapsed_vals else None

    verification_vals = [r["verification_ms"] for r in rows if r["verification_ms"] is not None]
    verification_summary = {
        "median": percentile(verification_vals, 0.50),
        "p95": percentile(verification_vals, 0.95),
    } if verification_vals else None

    has_context = any(r.get("context_bytes") is not None for r in rows)
    context_summary = {
        "total": sum(r.get("context_bytes") or 0 for r in rows)
    } if has_context else None

    measured_tokens = [r["token_cost"] for r in rows if r["token_cost"] is not None]
    if len(measured_tokens) == count and count > 0:
        usage_status = "measured"
        total_tokens = sum(measured_tokens)
    elif measured_tokens:
        usage_status = "partial"
        total_tokens = sum(measured_tokens)
    else:
        usage_status = "unavailable"
        total_tokens = None
    token_summary = {"total": total_tokens, "status": usage_status}

    measured_currency = [r["currency_cost"] for r in rows if r["currency_cost"] is not None]
    currency_total = round(sum(measured_currency), 4) if measured_currency else None

    build_vals = [r.get("build_ms") for r in rows if r.get("build_ms") is not None]
    build_summary = {
        "median": percentile(build_vals, 0.50),
        "p95": percentile(build_vals, 0.95),
    } if build_vals else None

    return {"scenario_count": count, "successes": success, "failures": count - success,
            "first_attempt_pass_rate": first_passes / count,
            "first_attempt_95_interval": interval(first_passes, count),
            "final_success_rate": success / count, "final_success_95_interval": interval(success, count),
            "regression_rate": sum(r["regression_seen"] for r in rows) / count,
            "attempts": attempts,
            "repair_cycles": repair_cycles,
            "verification_cost": verification_cost,
            "elapsed_ms": elapsed_summary,
            "verification_ms": verification_summary,
            "build_ms": build_summary,
            "context_bytes": context_summary,
            "token_cost": token_summary,
            "currency_cost": {"total": currency_total}}
