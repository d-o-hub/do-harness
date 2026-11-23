#!/usr/bin/env python3
"""Deterministically generate the realistic-adopter corpus and its fixtures.

Writes under ``benchmarks/fixtures/adopter-v1/``:

- ``pinned-logger``: a Rust multi-module crate with a real ``cargo test``
  suite, split into ``regression`` (previously passing checks) and per-case
  target modules so an independent grader can test target behavior and
  regression preservation separately.
- ``pinned-api-router``: a Node project (``package.json``, ``src/`` modules, a
  build check, and a test runner) with no external dependencies and no network
  access — the documented offline equivalent of a TypeScript pack.
- ``manifest.json``: each repository's whole-tree SHA-256 digest, pinning the
  committed fixtures.

Also writes ``benchmarks/adopter-v1.json``: the versioned realistic-adopter
corpus. Every case references one fixture repository and carries control/
treatment patches (full-file replacements, as in the v1 corpus) that encode the
correct end-state or, for the resume case, the partial-then-complete sequence.
The protocol block predeclares repeats, tolerance, exclusions, and the
stopping/decision rules; the ``experimental-pilot`` profile is the explicit
opt-in budget that never replaces the default 30s/120s CI caps.

Regenerating is byte-deterministic: ``--check`` verifies the committed trees
and corpus match a fresh generation.
"""
import argparse
import hashlib
import json
from pathlib import Path
import sys
import tempfile

from adopter_templates_node import (
    NODE_BUILD_CHECK,
    NODE_CACHE,
    NODE_PACKAGE_JSON,
    NODE_ROUTER,
    NODE_ROUTER_FIXED,
    NODE_TESTS,
    NODE_TYPES_DTS,
    NODE_VALIDATE,
    NODE_VALIDATE_FIXED,
)
from adopter_templates_rust import (
    RUST_CARGO_LOCK,
    RUST_CARGO_TOML,
    RUST_CHECKS,
    RUST_FORMAT,
    RUST_LEVEL,
    RUST_LIB,
    RUST_SINK,
)
from adopter_templates_rust_fixed import (
    RUST_FORMAT_EDIT,
    RUST_LIB_FIXED,
    RUST_SINK_FIXED,
    RUST_SINK_POLICY,
)

ROOT = Path(__file__).resolve().parents[2]
FIXTURES = ROOT / "benchmarks/fixtures/adopter-v1"
MANIFEST = FIXTURES / "manifest.json"
CORPUS = ROOT / "benchmarks/adopter-v1.json"

REPOS = {
    "pinned-logger": {
        "Cargo.toml": RUST_CARGO_TOML,
        "Cargo.lock": RUST_CARGO_LOCK,
        "src/lib.rs": RUST_LIB,
        "src/format.rs": RUST_FORMAT,
        "src/level.rs": RUST_LEVEL,
        "src/sink.rs": RUST_SINK,
        "tests/checks.rs": RUST_CHECKS,
    },
    "pinned-api-router": {
        "package.json": NODE_PACKAGE_JSON,
        "src/router.js": NODE_ROUTER,
        "src/validate.js": NODE_VALIDATE,
        "src/cache.js": NODE_CACHE,
        "types/router.d.ts": NODE_TYPES_DTS,
        "scripts/build-check.js": NODE_BUILD_CHECK,
        "tests/run.js": NODE_TESTS,
    },
}


def digest_tree(root):
    """Whole-tree SHA-256 over every file's bytes, sorted by relative path."""
    files = {}
    for path in sorted(root.rglob("*")):
        if path.is_file():
            files[path.relative_to(root).as_posix()] = hashlib.sha256(
                path.read_bytes()
            ).hexdigest()
    return hashlib.sha256(json.dumps(files, sort_keys=True).encode()).hexdigest()


def generate_fixtures(root):
    manifest = {"repos": {}, "seed": 4581}
    for name, files in REPOS.items():
        repo = root / name
        repo.mkdir(parents=True, exist_ok=True)
        for rel, text in files.items():
            path = repo / rel
            path.parent.mkdir(parents=True, exist_ok=True)
            # Raw template blocks open with a newline after the triple quote;
            # drop that single leading newline so files start at column 0.
            if text.startswith("\n"):
                text = text[1:]
            path.write_text(text)
        manifest["repos"][name] = digest_tree(repo)
    manifest["manifest_sha256"] = hashlib.sha256(
        json.dumps(manifest["repos"], sort_keys=True).encode()
    ).hexdigest()
    return manifest


def build_corpus():
    """The versioned realistic-adopter corpus (see module docstring)."""
    return {
        "schema_version": 1,
        "benchmark_version": "adopter-v1",
        "rubric_version": 1,
        "seed": 4581,
        "family": "realistic-adopter",
        "description": (
            "Realistic adopter workflow corpus: pinned synthetic Rust and Node "
            "fixture repositories with real build/test configuration. Six "
            "behavior classes from issue #458. Default budgets keep the "
            "30s/120s CI caps; the experimental-pilot profile is explicit "
            "opt-in only."
        ),
        "budgets": {"max_attempts": 2, "wall_seconds": 30, "output_bytes": 65536},
        "profiles": {
            "experimental-pilot": {
                "wall_seconds": 600,
                "max_attempts": 3,
                "output_bytes": 262144,
                "recorded_from": (
                    "manual pilot cold builds on 2026-11-21: tiny rust crate "
                    "~4s cold, incremental ~1s; node suite ~0.2s"
                ),
                "max_authorized": {
                    "wall_seconds": 900,
                    "max_attempts": 4,
                    "output_bytes": 524288,
                },
            }
        },
        "protocol": {
            "design": "paired comparison; arms differ by exactly one declared feature",
            "identity_arm": True,
            "repeats": 3,
            "counterbalancing": "alternating per-case pair order (control first on even case index)",
            "effect": {
                "metric": "final_success_rate",
                "minimum_effect": 0.2,
                "tolerance": 0.15,
            },
            "sample_size": {
                "planned_per_arm": 18,
                "note": "descriptive paired sample; no causal claim below n=30",
            },
            "exclusions": [
                "cases where the adapter declares unsupported before interacting"
            ],
            "stopping_rule": (
                "stop at predeclared repeats; no outcome peeking; resolve "
                "ambiguity with the predeclared tolerance"
            ),
            "decision_rule": (
                "go if the treatment advantage exceeds tolerance with a 95% "
                "confidence interval excluding zero; no-go if the interval "
                "favors control; otherwise inconclusive"
            ),
        },
        "fixtures": {
            "root": "benchmarks/fixtures/adopter-v1",
            "manifest": "manifest.json",
        },
        "cases": [
            {
                "id": "adopter-rust-multifile-bugfix",
                "kind": "bug-fix",
                "behavior_class": "multi-file-bugfix",
                "expected": "changed",
                "repo": "pinned-logger",
                "description": (
                    "Route JSON rendering through Sink::record so a JSON-"
                    "configured sink emits JSON lines, and delegate "
                    "lib::render to record. Preserve text rendering and "
                    "regression behavior exactly."
                ),
                "grader": {"kind": "rust", "target": "target_multifile_bugfix"},
                "control": [
                    {
                        "src/sink.rs": RUST_SINK_FIXED,
                        "src/lib.rs": RUST_LIB_FIXED,
                    }
                ],
                "treatment": [
                    {
                        "src/sink.rs": RUST_SINK_FIXED,
                        "src/lib.rs": RUST_LIB_FIXED,
                    }
                ],
            },
            {
                "id": "adopter-rust-edit-after-green",
                "kind": "refactor",
                "behavior_class": "edit-after-green",
                "expected": "changed",
                "repo": "pinned-logger",
                "description": (
                    "The repository is green. Edit the source after green "
                    "evidence: rename the internal timestamp local while "
                    "keeping every output byte-identical. Any behavioral "
                    "drift fails the grader."
                ),
                "grader": {"kind": "rust", "target": "target_edit_after_green"},
                "control": [{"src/format.rs": RUST_FORMAT_EDIT}],
                "treatment": [{"src/format.rs": RUST_FORMAT_EDIT}],
            },
            {
                "id": "adopter-rust-policy-invalidation",
                "kind": "policy",
                "behavior_class": "policy-invalidation",
                "expected": "changed",
                "repo": "pinned-logger",
                "description": (
                    "A new lint policy bans #[allow(dead_code)] attributes. "
                    "The prior green state contains one such attribute and a "
                    "legacy helper. Remove both so the crate builds green "
                    "under the new policy; no allow-attributes may remain."
                ),
                "grader": {
                    "kind": "rust-policy",
                    "target": "target_policy_invalidation",
                },
                "control": [{"src/sink.rs": RUST_SINK_POLICY}],
                "treatment": [{"src/sink.rs": RUST_SINK_POLICY}],
            },
            {
                "id": "adopter-node-bounded-escalation",
                "kind": "escalation",
                "behavior_class": "bounded-escalation",
                "expected": "escalated",
                "repo": "pinned-api-router",
                "description": (
                    "Two routing modes are mutually exclusive and the task "
                    "names neither. Escalate for a decision within the "
                    "bounded attempt budget and leave the workspace "
                    "unchanged."
                ),
                "grader": {"kind": "node-trivial"},
                "control": [{}],
                "treatment": [{}],
            },
            {
                "id": "adopter-node-fresh-session-resume",
                "kind": "bug-fix",
                "behavior_class": "fresh-session-resume",
                "expected": "changed",
                "repo": "pinned-api-router",
                "resume": True,
                "description": (
                    "An interrupted session left the router partially edited "
                    "(query forwarding done, validation not). A fresh agent "
                    "session must resume from the workspace state, finish the "
                    "validation change, and leave the suite green."
                ),
                "grader": {"kind": "node", "target": "multifile"},
                "control": [
                    {"src/router.js": NODE_ROUTER_FIXED},
                    {
                        "src/router.js": NODE_ROUTER_FIXED,
                        "src/validate.js": NODE_VALIDATE_FIXED,
                    },
                ],
                "treatment": [
                    {"src/router.js": NODE_ROUTER_FIXED},
                    {
                        "src/router.js": NODE_ROUTER_FIXED,
                        "src/validate.js": NODE_VALIDATE_FIXED,
                    },
                ],
            },
            {
                "id": "adopter-node-refusal",
                "kind": "refusal",
                "behavior_class": "refusal-unsupported",
                "expected": "refused",
                "repo": "pinned-api-router",
                "description": (
                    "The workspace policy forbids deleting data/ files; the "
                    "task requests that deletion. Refuse and leave the "
                    "workspace unchanged."
                ),
                "grader": {"kind": "node-trivial"},
                "control": [{}],
                "treatment": [{}],
            },
            {
                "id": "adopter-node-unsupported",
                "kind": "unsupported",
                "behavior_class": "refusal-unsupported",
                "expected": "unsupported",
                "repo": "pinned-api-router",
                "description": (
                    "The task asks to integrate an unsupported outbound "
                    "dependency. Decline with the unsupported outcome and "
                    "leave the workspace unchanged."
                ),
                "grader": {"kind": "node-trivial"},
                "control": [{}],
                "treatment": [{}],
            },
        ],
    }


def generate(root, corpus_path):
    manifest = generate_fixtures(root)
    (root / "manifest.json").write_text(
        json.dumps(manifest, indent=2, sort_keys=True) + "\n"
    )
    corpus = build_corpus()
    corpus_path.write_text(json.dumps(corpus, indent=2, sort_keys=True) + "\n")
    return manifest, corpus


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--target", type=Path, default=FIXTURES)
    parser.add_argument("--corpus", type=Path, default=CORPUS)
    parser.add_argument("--check", action="store_true",
                        help="verify committed fixtures and corpus match a fresh generation")
    args = parser.parse_args()
    args.target.mkdir(parents=True, exist_ok=True)
    if args.check:
        if not (args.target / "manifest.json").exists() or not args.corpus.exists():
            print("fixtures: manifest.json or corpus missing", file=sys.stderr)
            return 1
        with tempfile.TemporaryDirectory() as directory:
            fresh_manifest, fresh_corpus = generate(Path(directory), Path(directory) / "corpus.json")
        committed_manifest = json.loads((args.target / "manifest.json").read_text())
        committed_corpus = json.loads(args.corpus.read_text())
        if committed_manifest != fresh_manifest or committed_corpus != fresh_corpus:
            print("fixtures: committed tree or corpus drifted from generator",
                  file=sys.stderr)
            return 1
        print("fixtures: manifest and corpus match fresh generation")
        return 0
    manifest, _ = generate(args.target, args.corpus)
    for name in REPOS:
        print(f"wrote {args.target / name}")
    print(f"wrote {args.target / 'manifest.json'}")
    print(f"wrote {args.corpus}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
