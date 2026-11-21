"""Fresh-context review adapter: packet assembly, launch, sanitized record.

Explicit opt-in only: a review runs when the operator supplies an external
reviewer command (--reviewer-command-json) or a deterministic stub
(--stub). Nothing here ever wires a model run into a sensor or CI; the only
CI-wired entry is the hermetic self-test suite (scripts/tests/
test_fresh_context_review.py).
"""

import argparse
import json
import sys
from pathlib import Path

from fresh_context_review import packet as packet_mod
from fresh_context_review import reviewer as reviewer_mod

ROOT = Path(__file__).resolve().parent.parent.parent
STUB = Path(__file__).resolve().parent / "stub_reviewer.py"
SMOKE_FIXTURES = Path(__file__).resolve().parent / "fixtures" / "smoke"
SMOKE_CASES = SMOKE_FIXTURES / "cases.json"
SMOKE_RECORD = ROOT / "plans" / "fresh-context-review-smoke-record.json"
from fresh_context_review.stub_reviewer import SCENARIOS as STUB_SCENARIOS

EXPERIMENT = "fresh-context-review-pilot"

DISPOSITION_STATUSES = ("confirmed", "rejected", "unresolved")


def build_parser():
    parser = argparse.ArgumentParser(
        prog="fresh-context-review",
        description="Launch one fresh-context design review and record a "
                    "sanitized outcome, or run the deterministic smoke suite.",
    )
    parser.add_argument("--design-doc", metavar="PATH")
    parser.add_argument("--revision", metavar="REV", default="HEAD")
    parser.add_argument("--record-out", metavar="FILE")
    parser.add_argument("--packet-out", metavar="FILE")
    parser.add_argument("--objective")
    parser.add_argument("--intended-behavior")
    parser.add_argument("--constraint", action="append", default=[])
    parser.add_argument("--proposed-contract")
    parser.add_argument("--open-question", action="append", default=[])
    parser.add_argument("--source-path", action="append", default=[])
    parser.add_argument("--test-path", action="append", default=[])
    parser.add_argument("--doc-path", action="append", default=[])
    parser.add_argument("--reviewer-command-json")
    parser.add_argument("--stub", choices=STUB_SCENARIOS)
    parser.add_argument("--parent-marker")
    parser.add_argument("--limit-timeout", type=int)
    parser.add_argument("--limit-findings", type=int)
    parser.add_argument("--limit-output", type=int)
    parser.add_argument("--meta", default="{}")
    parser.add_argument("--smoke", action="store_true")
    parser.add_argument("--dispose", metavar="RECORD_FILE")
    parser.add_argument("--finding-id")
    parser.add_argument("--status", choices=DISPOSITION_STATUSES)
    parser.add_argument("--note")
    return parser

def reviewer_command(args):
    """Resolve the explicit reviewer command; None means no capability."""
    if args.stub:
        return [sys.executable, str(STUB), "--scenario", args.stub]
    if args.reviewer_command_json:
        try:
            cmd = json.loads(args.reviewer_command_json)
        except ValueError:
            raise ValueError("--reviewer-command-json must be a JSON argv list")
        if not isinstance(cmd, list) or not cmd or not all(
            isinstance(c, str) for c in cmd
        ):
            raise ValueError("--reviewer-command-json must be a JSON argv list")
        return cmd
    return None


def run_one_review(repo, args, case=None):
    """Assemble the packet, launch a fresh reviewer, return a record dict."""
    if case is not None:
        design_doc = case["design_doc"]
        revision = case.get("revision", "HEAD")
        review_id = case["id"]
        objective = case["objective"]
        intended = case.get("intended_behavior", "")
        constraints = case.get("constraints", [])
        contract = case["proposed_contract"]
        questions = case.get("open_questions", [])
        sources = case.get("source_paths", [])
        tests = case.get("test_paths", [])
        docs = case.get("doc_paths", [])
        timeout_s = case.get("limit_timeout", packet_mod.DEFAULT_TIMEOUT_S)
        max_findings = case.get("limit_findings",
                                packet_mod.DEFAULT_MAX_FINDINGS)
        max_output_bytes = case.get("limit_output",
                                    packet_mod.DEFAULT_MAX_OUTPUT_BYTES)
        marker = case.get("parent_marker")
    else:
        design_doc = args.design_doc
        revision = args.revision
        review_id = args.record_out
        objective = args.objective
        intended = args.intended_behavior
        constraints = args.constraint
        contract = args.proposed_contract
        questions = args.open_question
        sources = args.source_path
        tests = args.test_path
        docs = args.doc_path
        timeout_s = args.limit_timeout or packet_mod.DEFAULT_TIMEOUT_S
        max_findings = args.limit_findings or packet_mod.DEFAULT_MAX_FINDINGS
        max_output_bytes = args.limit_output or packet_mod.DEFAULT_MAX_OUTPUT_BYTES
        marker = args.parent_marker

    if not design_doc or not objective or not contract:
        raise ValueError(
            "--design-doc, --objective, and --proposed-contract are required "
            "(or a --smoke case definition)"
        )

    packet = packet_mod.build_packet(
        repo=repo,
        design_doc=design_doc,
        revision=revision,
        review_id=review_id,
        objective=objective,
        intended_behavior=intended,
        constraints=constraints,
        proposed_contract=contract,
        open_questions=questions,
        source_paths=sources,
        test_paths=tests,
        doc_paths=docs,
        timeout_s=timeout_s,
        max_findings=max_findings,
        max_output_bytes=max_output_bytes,
    )
    packet = packet_mod.attach_hash(packet)

    if args.packet_out:
        Path(args.packet_out).write_text(packet_mod.canonical_json(packet) + "\n")

    cmd = reviewer_command(args)
    if cmd is None:
        record = {
            "outcome": reviewer_mod.OUTCOME_NOT_RUN,
            "reason": "no reviewer capability configured; fresh-context "
                      "review is explicit opt-in",
        }
    else:
        record = reviewer_mod.run_reviewer(
            cmd,
            packet_mod.canonical_json(packet),
            repo=repo,
            timeout_s=timeout_s,
            max_output_bytes=max_output_bytes,
            max_findings=max_findings,
            marker=marker,
        )
        if args.stub is None and record["outcome"] in (
            reviewer_mod.OUTCOME_SUCCESS_FINDINGS,
            reviewer_mod.OUTCOME_ZERO_FINDINGS,
        ) and not marker:
            record["outcome"] = reviewer_mod.OUTCOME_UNVERIFIED
            record["reason"] = (
                "isolation unverified: external reviewer without a parent-only "
                "marker probe"
            )

    record["schema_version"] = packet_mod.SCHEMA_VERSION
    record["experiment"] = EXPERIMENT
    record["review_id"] = review_id
    record["packet_hash"] = packet["packet_hash"]
    record["packet"] = packet
    record["reviewer_mode"] = "stub" if args.stub else (
        "external" if cmd is not None else "none"
    )
    try:
        record["meta"] = json.loads(args.meta)
    except ValueError:
        record["meta"] = {}
    record["isolation"] = {
        "parent_conversation": "none supplied to reviewer process",
        "host_injected_instructions": (
            "cannot be excluded for external reviewer services; recorded as "
            "unverified unless native records prove otherwise"
        ),
        "marker_probe_configured": bool(marker),
        "marker_leaked": record.get("marker_leaked"),
    }
    record["dispositions"] = []
    return record


def write_record(record_path, record):
    if not record_path:
        raise ValueError("--record-out is required")
    Path(record_path).parent.mkdir(parents=True, exist_ok=True)
    Path(record_path).write_text(packet_mod.canonical_record(record))


def dispose(record_path, finding_id, status, note):
    record = json.loads(Path(record_path).read_text())
    findings = record.get("findings", [])
    if finding_id not in [f.get("id") for f in findings]:
        raise ValueError("finding %r not present in record" % finding_id)
    disposition = {"status": status}
    if note:
        disposition["note"] = note
    record["dispositions"] = [
        d for d in record.get("dispositions", []) if d.get("id") != finding_id
    ] + [{"id": finding_id, **disposition}]
    write_record(record_path, record)
    return record


def run_smoke(repo):
    cases = json.loads(SMOKE_CASES.read_text())
    results = []
    all_ok = True
    for case in cases["cases"]:
        args = argparse.Namespace(
            stub=case["scenario"],
            reviewer_command_json=None,
            parent_marker=case.get("parent_marker"),
            packet_out=None,
            meta="{}",
            design_doc=None,
            revision=None,
            record_out=None,
            objective=None,
            intended_behavior=None,
            constraint=[],
            proposed_contract=None,
            open_question=[],
            source_path=[],
            test_path=[],
            doc_path=[],
            limit_timeout=None,
            limit_findings=None,
            limit_output=None,
        )
        record = run_one_review(repo, args, case=case)
        expected = case["expected_outcome"]
        ok = record["outcome"] == expected
        if case.get("expected_findings") is not None:
            ok = ok and record["findings_count"] == case["expected_findings"]
        if case.get("expected_finding_ids") is not None:
            actual_ids = [f.get("id") for f in record.get("findings", [])]
            ok = ok and actual_ids == case["expected_finding_ids"]
        if case.get("expected_marker_leaked") is not None:
            ok = ok and bool(record.get("marker_leaked")) == case["expected_marker_leaked"]
        all_ok = all_ok and ok
        results.append({
            "case": case["id"],
            "scenario": case["scenario"],
            "expected_outcome": expected,
            "outcome": record["outcome"],
            "expected_findings": case.get("expected_findings"),
            "findings_count": record.get("findings_count"),
            "expected_finding_ids": case.get("expected_finding_ids"),
            "finding_ids": [f.get("id") for f in record.get("findings", [])],
            "marker_leaked": record.get("marker_leaked"),
            "elapsed_seconds": record.get("elapsed_seconds"),
            "pass": ok,
        })
    summary = {
        "schema_version": packet_mod.SCHEMA_VERSION,
        "experiment": EXPERIMENT,
        "suite": "deterministic stub smoke (adapter mechanics only)",
        "isolation_note": (
            "Stub runs exercise packet/launch/outcome plumbing; they do not "
            "prove model context isolation or defect-detection benefit."
        ),
        "usage": {
            "currency_cost_usd": None,
            "token_count": None,
            "note": "no model or token usage in stub mode",
        },
        "results": results,
        "all_pass": all_ok,
    }
    if SMOKE_RECORD.parent != Path.cwd():
        SMOKE_RECORD.parent.mkdir(parents=True, exist_ok=True)
    SMOKE_RECORD.write_text(packet_mod.canonical_record(summary))
    return 0 if all_ok else 1


def main(argv=None):
    parser = build_parser()
    args = parser.parse_args(argv)
    repo = str(ROOT)

    if args.dispose:
        try:
            dispose(args.dispose, args.finding_id, args.status, args.note)
        except (ValueError, KeyError, OSError) as exc:
            print("fresh-context-review: %s" % exc, file=sys.stderr)
            return 2
        return 0

    if args.smoke:
        try:
            return run_smoke(repo)
        except (ValueError, OSError) as exc:
            print("fresh-context-review: smoke failed: %s" % exc,
                  file=sys.stderr)
            return 2

    try:
        record = run_one_review(repo, args)
        write_record(args.record_out, record)
    except (ValueError, OSError) as exc:
        print("fresh-context-review: %s" % exc, file=sys.stderr)
        return 2
    print("outcome=%s review_id=%s" % (record["outcome"], record["review_id"]))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
