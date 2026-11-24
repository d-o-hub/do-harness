"""Evaluation corpus loading, validation, and packet assembly for #499.

The corpus is the frozen, reviewer-facing case table. Ground truth lives in a
per-case ``ground-truth.json`` file that packet assembly must never include;
any reference to a forbidden oracle path is a hard error (fail closed), so a
leaked oracle cannot silently reach a reviewer.
"""

import hashlib
import json
from pathlib import Path

SCHEMA_VERSION = 1
EXPERIMENT = "fresh-context-review-decision-v1"
RISK_CLASSES = ("authorization", "migration", "api-compat")
VARIANTS = ("flawed", "sound", "control")
ORACLE_FILENAME = "ground-truth.json"
FORBIDDEN_PATH_MARKERS = ("ground-truth", "oracle", "grader")
DEFAULT_LIMITS = {
    "per_review_timeout_s": 300,
    "max_findings_per_review": 5,
    "max_output_bytes_per_review": 65536,
    "max_packet_bytes": 1048576,
}


def canonical_json(obj):
    return json.dumps(obj, sort_keys=True, separators=(",", ":"),
                      ensure_ascii=False)


def sha256_hex(raw):
    return "sha256:" + hashlib.sha256(raw.encode("utf-8")).hexdigest()


def packet_hash(packet):
    body = {k: v for k, v in packet.items() if k != "packet_hash"}
    return sha256_hex(canonical_json(body))


def _reject_forbidden_path(path, where):
    lowered = path.lower()
    for marker in FORBIDDEN_PATH_MARKERS:
        if marker in lowered:
            raise ValueError(
                "%s references forbidden oracle path %r (marker %r)"
                % (where, path, marker)
            )


def load_cases(root):
    """Load and validate the case table; return (cases, root)."""
    root = Path(root)
    data = json.loads((root / "cases.json").read_text(encoding="utf-8"))
    if data.get("schema_version") != SCHEMA_VERSION:
        raise ValueError("invalid corpus schema_version")
    if data.get("experiment") != EXPERIMENT:
        raise ValueError("invalid corpus experiment")
    cases = data["cases"]
    ids = set()
    by_class = {}
    for case in cases:
        case_id = case["id"]
        if case_id in ids:
            raise ValueError("duplicate case id %r" % case_id)
        ids.add(case_id)
        if case["class"] not in RISK_CLASSES + ("routine-control",):
            raise ValueError("invalid class in %r" % case_id)
        if case["variant"] not in VARIANTS:
            raise ValueError("invalid variant in %r" % case_id)
        if case["variant"] != "control" and case["class"] != "routine-control":
            by_class.setdefault(case["class"], set()).add(case["variant"])
        design = root / case["design_doc"]
        if not design.is_file():
            raise ValueError("missing design doc in %r" % case_id)
        _reject_forbidden_path(case["design_doc"], "case %s" % case_id)
        for source in case.get("source_files", []):
            if not (root / source).is_file():
                raise ValueError("missing source file in %r" % case_id)
            _reject_forbidden_path(source, "case %s" % case_id)
        if not (root / case_id / ORACLE_FILENAME).is_file():
            raise ValueError("missing oracle for %r" % case_id)
        limits = case.get("limits", {})
        for key, value in limits.items():
            if key not in DEFAULT_LIMITS or type(value) is not int or value <= 0:
                raise ValueError("invalid per-case limit in %r" % case_id)
    for risk_class, variants in by_class.items():
        if variants != {"flawed", "sound"}:
            raise ValueError("class %s missing flawed/sound case" % risk_class)
    if not any(c["class"] == "routine-control" and c["variant"] == "control"
               for c in cases):
        raise ValueError("missing routine low-risk control case")
    return cases, root


def load_oracle(root, case):
    """Load and validate the frozen ground truth for one case."""
    oracle = json.loads(
        (root / case["id"] / ORACLE_FILENAME).read_text(encoding="utf-8")
    )
    if oracle.get("case_id") != case["id"]:
        raise ValueError("oracle case_id mismatch for %r" % case["id"])
    defects = oracle["defects"]
    defect_ids = [d["id"] for d in defects]
    if len(defect_ids) != len(set(defect_ids)):
        raise ValueError("duplicate defect ids in %r" % case["id"])
    expected = oracle["predeclared_expected_findings"]
    if any(d not in defect_ids for d in expected):
        raise ValueError("expected finding not a defect in %r" % case["id"])
    if case["variant"] == "flawed" and not expected:
        raise ValueError("flawed case %r must predeclare expected findings"
                         % case["id"])
    if case["variant"] in ("sound", "control") and expected:
        raise ValueError("sound/control case %r must predeclare zero findings"
                         % case["id"])
    if any(d not in defect_ids for d in oracle["new_beyond_baseline"]):
        raise ValueError("new_beyond_baseline references unknown defect in %r"
                         % case["id"])
    for defect in defects:
        if defect["severity"] not in ("material", "minor"):
            raise ValueError("invalid severity in %r" % case["id"])
        for key in ("summary", "trigger", "consequence", "evidence",
                    "smallest_correction"):
            if not isinstance(defect.get(key), str) or not defect[key].strip():
                raise ValueError("defect %r missing %s" % (defect["id"], key))
    for fp in oracle.get("false_positive_pool", []):
        if not isinstance(fp.get("id"), str) or not fp["id"].strip():
            raise ValueError("false positive pool entry without id in %r"
                             % case["id"])
    return oracle


def load_corpus(root):
    """Return {cases, oracles} for the whole evaluation corpus."""
    cases, root = load_cases(root)
    oracles = {case["id"]: load_oracle(root, case) for case in cases}
    return {"cases": cases, "oracles": oracles, "root": root}


def build_packet(case, root, review_id, revision="HEAD", limits=None):
    """Assemble a frozen reviewer packet with embedded design and source.

    Fails closed on any oracle reference; the packet never contains ground
    truth. Returns the packet dict with a stable packet_hash.
    """
    root = Path(root)
    merged_limits = dict(DEFAULT_LIMITS)
    merged_limits.update(limits or {})
    merged_limits.update(case.get("limits", {}))
    design = (root / case["design_doc"]).read_text(encoding="utf-8")
    design_path = case["design_doc"]
    _reject_forbidden_path(design_path, "packet assembly")
    sources = {}
    for source in case.get("source_files", []):
        _reject_forbidden_path(source, "packet assembly")
        sources[source] = (root / source).read_text(encoding="utf-8")
    packet = {
        "schema_version": SCHEMA_VERSION,
        "experiment": EXPERIMENT,
        "review_id": review_id,
        "case_id": case["id"],
        "class": case["class"],
        "variant": case["variant"],
        "objective": case["objective"],
        "intended_behavior": case["intended_behavior"],
        "constraints": list(case.get("constraints", [])),
        "proposed_contract": case["proposed_contract"],
        "open_questions": list(case.get("open_questions", [])),
        "paths": {
            "design_doc": design_path,
            "source_files": sorted(sources),
        },
        "revision": revision,
        "limits": {k: v for k, v in merged_limits.items()
                   if k != "max_packet_bytes"},
        "design": design,
        "sources": sources,
        "reviewer_note": (
            "Packet inputs are frozen; ground truth and expected findings are "
            "never part of a reviewer packet."
        ),
    }
    packet["packet_hash"] = packet_hash(packet)
    return packet


def packet_contains_oracle(packet):
    """True when serialized packet content includes oracle material."""
    text = canonical_json(packet)
    return any(marker in text.lower() for marker in FORBIDDEN_PATH_MARKERS)
