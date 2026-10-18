#!/usr/bin/env python3
"""
docs.py — Documentation inventory and link correctness checker.

Usage:
  python3 scripts/docs.py links --check [--root ROOT] [--contracts CONTRACTS] [--mode local|ci]
  python3 scripts/docs.py inventory [--root ROOT] [--contracts CONTRACTS] [--mode local|ci]
  python3 scripts/docs.py sync --check|--write [--root ROOT]
"""

import argparse
import os
import sys

from doc_tools import cli_sync, examples
from doc_tools.inventory import discover_documents
from doc_tools.links import LinkChecker


def main():
    parser = argparse.ArgumentParser(description="Documentation inventory and link correctness tool")
    subparsers = parser.add_subparsers(dest="subcommand", required=True)

    links_parser = subparsers.add_parser("links", help="Check document link correctness")
    links_parser.add_argument("--check", action="store_true", help="Perform offline link and anchor verification")
    links_parser.add_argument("--root", default=".", help="Root directory of repository")
    links_parser.add_argument("--contracts", default="docs/contracts.toml", help="Path to contracts policy file")
    links_parser.add_argument("--mode", choices=["local", "ci"], default="local", help="Discovery mode")

    inv_parser = subparsers.add_parser("inventory", help="Inspect documentation inventory")
    inv_parser.add_argument("--root", default=".", help="Root directory of repository")
    inv_parser.add_argument("--contracts", default="docs/contracts.toml", help="Path to contracts policy file")
    inv_parser.add_argument("--mode", choices=["local", "ci"], default="local", help="Discovery mode")

    examples_parser = subparsers.add_parser("examples", help="Verify documented CLI examples against real execution/fixtures")
    examples_parser.add_argument("--check", action="store_true", help="Perform example execution and verification")
    examples_parser.add_argument("--root", default=".", help="Root directory of repository")
    examples_parser.add_argument("--contracts", default="docs/contracts.toml", help="Path to contracts policy file")

    sync_parser = subparsers.add_parser("sync", help="Check or regenerate CLI-derived documentation blocks")
    sync_parser.add_argument("--check", action="store_true", help="Verify committed blocks against the CLI definitions (default)")
    sync_parser.add_argument("--write", action="store_true", help="Rewrite out-of-date blocks in place (refused while CI is set)")
    sync_parser.add_argument("--root", default=".", help="Root directory of repository")

    impact_parser = subparsers.add_parser("impact", help="Perform deterministic docs impact analysis and emit review packets")
    impact_parser.add_argument("--base", required=True, help="Base revision/branch or merge base commit")
    impact_parser.add_argument("--head", required=True, help="Head revision/branch or commit")
    impact_parser.add_argument("--format", choices=["json", "text", "packet"], default="json", help="Output format (default: json)")
    impact_parser.add_argument("--root", default=".", help="Root directory of repository")
    impact_parser.add_argument("--contracts", default="docs/contracts.toml", help="Path to contracts policy file")

    args = parser.parse_args()

    root_dir = os.path.abspath(args.root)

    if args.subcommand == "sync":
        if args.check and args.write:
            print("sync: --check and --write are mutually exclusive", file=sys.stderr)
            sys.exit(2)
        sys.exit(cli_sync.run_sync(root_dir, write=args.write))

    if args.subcommand == "examples":
        ok = examples.verify_examples(root_dir=root_dir)
        sys.exit(0 if ok else 1)

    contracts_path = os.path.join(root_dir, args.contracts) if not os.path.isabs(args.contracts) else args.contracts

    if args.subcommand == "inventory":
        try:
            res = discover_documents(root_dir, contracts_path, mode=args.mode)
            print(f"Discovered {len(res['records'])} total document(s) in contracts policy:")
            print(f"  - Maintained: {len(res['maintained'])}")
            print(f"  - Canonical Aliases: {len(res['canonical_aliases'])}")
            print(f"  - Historical: {len(res['historical'])}")
            print(f"  - Vendored: {len(res['vendored'])}")
            print(f"  - Fixtures: {len(res['fixture'])}")
            sys.exit(0)
        except Exception as e:
            print(f"Inventory failure: {e}", file=sys.stderr)
            sys.exit(1)

    if args.subcommand == "impact":
        from doc_tools.report import build_doc_report_and_packet
        try:
            report, packet_text = build_doc_report_and_packet(
                root_dir=root_dir,
                base_rev=args.base,
                head_rev=args.head,
                contracts_path=args.contracts,
            )
            if args.format == "json":
                import json
                # Strip policy object before serializing
                report_out = dict(report)
                print(json.dumps(report_out, indent=2))
            elif args.format == "packet":
                print(packet_text)
            else:
                print(f"Base: {report['revisions']['base_rev']} ({report['revisions']['merge_base'][:8]})")
                print(f"Head: {report['revisions']['head_rev']} ({report['revisions']['head_commit'][:8]})")
                print(f"Mechanical Status: {'PASS' if not report['mechanical']['findings'] else 'FAIL'}")
                print(f"Affected Docs: {len(report['impact']['affected_doc_sections'])}")
                print(f"Packet Size: {report['packet_summary']['packet_bytes']} bytes")
            sys.exit(0)
        except Exception as e:
            print(f"Impact analysis error: {e}", file=sys.stderr)
            sys.exit(1)

    if args.subcommand == "links":
        checker = LinkChecker(root_dir=root_dir, contracts_path=contracts_path, mode=args.mode)
        checker.check_links()

        if checker.warnings:
            for w in checker.warnings:
                print(w)

        if checker.errors:
            for e in checker.errors:
                print(e, file=sys.stderr)
            print(f"\nLink check FAILED: {len(checker.errors)} error(s) found.", file=sys.stderr)
            sys.exit(1)

        print("Link check OK: all local links and anchors are valid.")
        sys.exit(0)


if __name__ == "__main__":
    main()
