#!/usr/bin/env python3
"""Machine-readable workflow cost report generator and baseline analyzer for CI.

Analyzes workflow cost baselines across `verify.yml`, `scheduled.yml`,
`semver.yml`, and `release.yml`, highlights top cost contributors, quantifies
proven savings from duplicate work removal, and verifies coverage retention.

This tool does NOT claim durable DORA history from ephemeral runner-local state.
"""

import argparse
import json
import pathlib
import sys

ROOT = pathlib.Path(__file__).resolve().parents[1]
DEFAULT_BASELINE = ROOT / "plans/ci-workflow-costs.json"

DISCLAIMER = (
    "This report records workflow step timing and cost baselines from CI runners. "
    "It does not claim durable DORA history from ephemeral runner-local state."
)


def load_baseline(path: pathlib.Path) -> dict:
    if not path.is_file():
        raise FileNotFoundError(f"Baseline dataset file not found: {path}")
    with path.open("r", encoding="utf-8") as f:
        data = json.load(f)
    if "baseline_metrics" not in data or "top_cost_contributors" not in data:
        raise ValueError(f"Invalid baseline schema in {path}")
    return data


def calculate_cost_report(data: dict) -> dict:
    baseline_metrics = data.get("baseline_metrics", {})
    proven_optimizations = data.get("proven_optimizations", [])

    total_baseline_seconds = 0
    workflow_summaries = {}

    for wf_name, wf_data in baseline_metrics.items():
        wf_total = 0
        jobs_summary = {}
        for job_name, job_data in wf_data.get("jobs", {}).items():
            duration = job_data.get("median_duration_seconds", 0)
            wf_total += duration
            jobs_summary[job_name] = {
                "runner": job_data.get("runner", "unknown"),
                "median_duration_seconds": duration,
                "steps_count": len(job_data.get("steps", {})),
            }
        workflow_summaries[wf_name] = {
            "total_median_duration_seconds": wf_total,
            "jobs": jobs_summary,
        }
        total_baseline_seconds += wf_total

    total_estimated_savings_seconds = 0
    total_measured_savings_seconds = 0

    for opt in proven_optimizations:
        kind = opt.get("metric_kind", "historical_estimate")
        est = opt.get("estimated_savings_seconds", 0)
        meas = opt.get("measured_savings_seconds", 0)
        if kind == "historical_estimate" or est > 0:
            total_estimated_savings_seconds += est
        if kind != "historical_estimate" and meas > 0:
            total_measured_savings_seconds += meas

    projected_post_change_seconds = max(
        0, total_baseline_seconds - total_estimated_savings_seconds
    )
    savings_percentage = (
        (total_estimated_savings_seconds / total_baseline_seconds * 100.0)
        if total_baseline_seconds > 0
        else 0.0
    )

    return {
        "disclaimer": DISCLAIMER,
        "sample_metadata": data.get("sample_metadata", {}),
        "total_baseline_duration_seconds": total_baseline_seconds,
        "total_runner_duration_seconds": total_baseline_seconds,
        "total_proven_savings_seconds": total_measured_savings_seconds,
        "total_estimated_savings_seconds": total_estimated_savings_seconds,
        "total_measured_savings_seconds": total_measured_savings_seconds,
        "projected_duration_seconds": projected_post_change_seconds,
        "savings_percentage": round(savings_percentage, 1),
        "workflows": workflow_summaries,
        "top_cost_contributors": data.get("top_cost_contributors", []),
        "proven_optimizations": proven_optimizations,
        "retained_checks_matrix": data.get("retained_checks_matrix", {}),
    }


def format_text_report(report: dict) -> str:
    lines = []
    lines.append("=======================================================================")
    lines.append("                  CI WORKFLOW COST & SAVINGS REPORT                    ")
    lines.append("=======================================================================")
    lines.append(f"Disclaimer: {report['disclaimer']}")
    lines.append("")

    meta = report.get("sample_metadata", {})
    lines.append(
        f"Sampled Runs: {meta.get('sampled_runs', 0)} ({meta.get('successful_runs', 0)} successful, {meta.get('failed_runs', 0)} failed)"
    )
    lines.append(f"Workflows: {', '.join(meta.get('workflows', []))}")
    lines.append("")

    lines.append("--- SUMMARY METRICS ---")
    lines.append(f"Baseline Total Runner Compute: {report['total_runner_duration_seconds']} runner-seconds")
    lines.append(f"Baseline Median Duration Total: {report['total_baseline_duration_seconds']}s")
    lines.append(
        f"Estimated / Projected Savings: {report['total_estimated_savings_seconds']}s ({report['savings_percentage']}%) (projection)"
    )
    lines.append(f"Measured Savings: {report.get('total_measured_savings_seconds', 0)}s")
    lines.append(f"Projected Median Duration Total: {report['projected_duration_seconds']}s")
    lines.append("")

    lines.append("--- TOP COST CONTRIBUTORS ---")
    for item in report.get("top_cost_contributors", []):
        lines.append(f"#{item.get('rank', '?')} [{item.get('workflow', 'all')}] {item.get('category', 'unknown')}")
        lines.append(f"   Impact: {item.get('impact', '')}")
    lines.append("")

    lines.append("--- OPTIMIZATIONS & COVERAGE PRESERVATION ---")
    for opt in report.get("proven_optimizations", []):
        kind = opt.get("metric_kind", "historical_estimate")
        if kind == "measured_runner_seconds" or "attributable_runs" in opt:
            runs = opt.get("attributable_runs", {})
            before_r = runs.get("before_run_id", "?")
            after_r = runs.get("after_run_id", "?")
            before_ts = runs.get("before_timestamp", "?")
            after_ts = runs.get("after_timestamp", "?")
            savings = opt.get("measured_savings_seconds", opt.get("estimated_savings_seconds", 0))
            lines.append(
                f"* {opt.get('id', 'opt')}: -{savings}s [MEASURED: before run {before_r} @ {before_ts} vs after run {after_r} @ {after_ts}]"
            )
        else:
            savings = opt.get("estimated_savings_seconds", 0)
            lines.append(f"* {opt.get('id', 'opt')}: -{savings}s [HISTORICAL ESTIMATE / PROJECTION]")
        lines.append(f"  Description: {opt.get('description', '')}")
        lines.append(f"  Coverage Preserved: {opt.get('coverage_preserved', '')}")
    lines.append("")
    lines.append("--- RETAINED CHECKS MATRIX ---")
    for check, details in report.get("retained_checks_matrix", {}).items():
        lines.append(f"* {check}: {details}")
    lines.append("=======================================================================")

    return "\n".join(lines)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--baseline",
        type=pathlib.Path,
        default=DEFAULT_BASELINE,
        help="Path to baseline metrics JSON dataset.",
    )
    parser.add_argument(
        "--format",
        choices=("text", "json"),
        default="text",
        help="Output report format (text or json).",
    )
    parser.add_argument(
        "--check",
        action="store_true",
        help="Check baseline dataset validity and exit 0 if valid.",
    )

    args = parser.parse_args()

    try:
        data = load_baseline(args.baseline)
        report = calculate_cost_report(data)
    except Exception as e:
        print(f"FAIL: ci_workflow_costs: {e}", file=sys.stderr)
        sys.exit(1)

    if args.check:
        if report["total_baseline_duration_seconds"] <= 0:
            print("FAIL: total_baseline_duration_seconds must be > 0", file=sys.stderr)
            sys.exit(1)
        if report["total_estimated_savings_seconds"] < 0:
            print("FAIL: total_estimated_savings_seconds cannot be negative", file=sys.stderr)
            sys.exit(1)
        if report["total_measured_savings_seconds"] < 0:
            print("FAIL: total_measured_savings_seconds cannot be negative", file=sys.stderr)
            sys.exit(1)
        print("OK: ci_workflow_costs check passed.")
        sys.exit(0)

    if args.format == "json":
        print(json.dumps(report, indent=2))
    else:
        print(format_text_report(report))


if __name__ == "__main__":
    main()
