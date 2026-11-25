"""Fresh-context review packet assembly, canonicalization, hashing, validation.

A packet is a frozen, self-describing review input: the objective, intended
behavior, explicit constraints, proposed contract, relevant paths with pinned
revisions, open questions, and predeclared limits. The packet identity is a
SHA-256 over the canonical JSON of every field except the hash itself, so the
identity is stable for identical inputs and changes when any input changes.

All source/test/doc paths are resolved to git blob revisions at the pinned
commit; a missing file at that revision is a hard error, never a silently
dropped input.
"""

import hashlib
import json
import subprocess  # nosec B404 - fixed git argv only, never a shell

SCHEMA_VERSION = 1
DEFAULT_TIMEOUT_S = 300
DEFAULT_MAX_FINDINGS = 5
DEFAULT_MAX_OUTPUT_BYTES = 65536

# Top-level packet fields, in the order used by the canonical record writer.
PACKET_FIELDS = (
    "schema_version",
    "review_id",
    "objective",
    "intended_behavior",
    "constraints",
    "proposed_contract",
    "open_questions",
    "paths",
    "revisions",
    "limits",
    "reviewer_note",
)


def canonical_json(obj):
    """Deterministic JSON: sorted keys, compact separators, UTF-8."""
    return json.dumps(obj, sort_keys=True, separators=(",", ":"), ensure_ascii=False)


def sha256_hex(raw):
    return "sha256:" + hashlib.sha256(raw.encode("utf-8")).hexdigest()


def git_output(repo, args):
    # The argv is a fixed `git -C <repo>` list with no shell; `repo` and
    # `args` come from this repository's own pinned revision lookups.
    # nosemgrep: python.lang.security.audit.dangerous-subprocess-use-audit.dangerous-subprocess-use-audit
    proc = subprocess.run(  # nosec B603 - fixed git argv, no shell
        ["git", "-C", repo, *args],
        capture_output=True,
        text=True,
    )
    if proc.returncode != 0:
        raise ValueError(
            "git %s failed: %s" % (" ".join(args), proc.stderr.strip()[:300])
        )
    return proc.stdout.strip()


def commit_sha(repo, revision):
    return git_output(repo, ["rev-parse", "%s^{commit}" % revision])


def blob_sha(repo, revision, path):
    return git_output(repo, ["rev-parse", "%s:%s" % (revision, path)])


def build_packet(
    *,
    repo,
    design_doc,
    revision,
    review_id,
    objective,
    intended_behavior,
    constraints,
    proposed_contract,
    open_questions,
    source_paths,
    test_paths,
    doc_paths,
    timeout_s=DEFAULT_TIMEOUT_S,
    max_findings=DEFAULT_MAX_FINDINGS,
    max_output_bytes=DEFAULT_MAX_OUTPUT_BYTES,
):
    """Freeze a review packet against a git revision.

    Raises ValueError when the revision is unresolvable or any listed path
    does not exist at that revision (revision pinning fails closed).
    """
    if not objective.strip():
        raise ValueError("objective must be non-empty")
    if not proposed_contract.strip():
        raise ValueError("proposed_contract must be non-empty")
    if timeout_s <= 0 or max_findings <= 0 or max_output_bytes <= 0:
        raise ValueError("limits must be positive")

    commit = commit_sha(repo, revision)
    design_blob = blob_sha(repo, revision, design_doc)
    files = {}
    for path in (*source_paths, *test_paths, *doc_paths):
        files[path] = blob_sha(repo, revision, path)

    packet = {
        "schema_version": SCHEMA_VERSION,
        "review_id": review_id,
        "objective": objective,
        "intended_behavior": intended_behavior,
        "constraints": list(constraints),
        "proposed_contract": proposed_contract,
        "open_questions": list(open_questions),
        "paths": {
            "source": list(source_paths),
            "tests": list(test_paths),
            "docs": list(doc_paths),
        },
        "revisions": {
            "commit": commit,
            "design_doc": {"path": design_doc, "blob": design_blob},
            "files": files,
        },
        "limits": {
            "timeout_s": timeout_s,
            "max_findings": max_findings,
            "max_output_bytes": max_output_bytes,
        },
        "reviewer_note": (
            "The proposed contract and decision rationale live in the frozen "
            "design document itself. No author deliberation transcript, chat "
            "log, or summary telling the reviewer which conclusion to reach "
            "is included in this packet."
        ),
    }
    return packet


def packet_hash(packet):
    """Stable identity for a packet, excluding the hash field itself."""
    body = {k: v for k, v in packet.items() if k != "packet_hash"}
    return sha256_hex(canonical_json(body))


def attach_hash(packet):
    packet = dict(packet)
    packet["packet_hash"] = packet_hash(packet)
    return packet


def canonical_record(record):
    """Deterministic serialization for committed record files."""
    return canonical_json(record) + "\n"
