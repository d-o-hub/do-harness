#!/usr/bin/env python3
"""Fresh-context design-review decision machinery (issue #499).

Runs deterministic checks only: corpus validation, oracle-leak rejection, and
verdict aggregation over recorded runs. No model or network is ever invoked;
held-out effectiveness runs are explicit opt-in external-agent work gated on
the #498 pilot landing.
"""
import argparse
import json
import sys
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parent
sys.path.insert(0, str(ROOT))

from fresh_context_decision import aggregate as aggregate_mod
from fresh_context_decision import corpus as corpus_mod
from fresh_context_decision import tests as tests_mod

DEFAULT_CORPUS = ROOT.parent / "benchmarks" / "fresh-context-decision-v1"
DEFAULT_PROTOCOL = DEFAULT_CORPUS / "protocol.json"


def check_corpus(args):
    try:
        data = corpus_mod.load_corpus(args.corpus)
    except (ValueError, KeyError, OSError) as exc:
        print("fresh-context-decision: invalid corpus: %s" % exc,
              file=sys.stderr)
        return 2
    summary = {
        "schema_version": corpus_mod.SCHEMA_VERSION,
        "experiment": corpus_mod.EXPERIMENT,
        "case_count": len(data["cases"]),
        "classes": sorted({c["class"] for c in data["cases"]}),
        "all_oracles_predeclared": True,
    }
    print(json.dumps(summary, sort_keys=True))
    return 0


def aggregate_records(args):
    try:
        protocol = aggregate_mod.load_protocol(args.protocol)
        corpus = corpus_mod.load_corpus(args.corpus)
        records = []
        for path in sorted(Path(args.records).glob("*.json")):
            records.append(json.loads(path.read_text(encoding="utf-8")))
        report = aggregate_mod.aggregate(records, corpus, protocol)
    except (ValueError, KeyError, OSError) as exc:
        print("fresh-context-decision: aggregation failed: %s" % exc,
              file=sys.stderr)
        return 2
    print(json.dumps(report, sort_keys=True))
    return 0


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--self-test", action="store_true",
                        help="run the deterministic self-test suite")
    sub = parser.add_subparsers(dest="command")
    check = sub.add_parser("check-corpus")
    check.add_argument("--corpus", type=Path, default=DEFAULT_CORPUS)
    agg = sub.add_parser("aggregate")
    agg.add_argument("--records", type=Path, required=True)
    agg.add_argument("--corpus", type=Path, default=DEFAULT_CORPUS)
    agg.add_argument("--protocol", type=Path, default=DEFAULT_PROTOCOL)
    args = parser.parse_args(argv)
    if args.self_test:
        result = unittest.TextTestRunner().run(
            unittest.defaultTestLoader.loadTestsFromModule(tests_mod)
        )
        return int(not result.wasSuccessful())
    if args.command == "check-corpus":
        return check_corpus(args)
    if args.command == "aggregate":
        return aggregate_records(args)
    parser.print_help()
    return 2


if __name__ == "__main__":
    raise SystemExit(main())
