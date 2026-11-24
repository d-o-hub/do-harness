"""Run-record schema for the #499 decision experiment.

A run record captures one arm of one paired session. Fields deliberately
reuse the #458 runner provenance vocabulary (token_cost, currency_cost,
context_bytes, usage_reason, verification_cost) and the #498 pilot outcome
and disposition vocabulary (success_with_findings / success_zero_findings /
failure / timeout / not_run / unverified; confirmed / rejected /
unresolved), so a recorded measurement can be combined across the shared
machinery without translation.
"""

from .corpus import EXPERIMENT, SCHEMA_VERSION

ARMS = ("baseline", "treatment")
OUTCOMES = (
    "success_with_findings",
    "success_zero_findings",
    "failure",
    "timeout",
    "not_run",
    "unverified",
)
COMPLETED_OUTCOMES = ("success_with_findings", "success_zero_findings")
DISPOSITION_STATUSES = ("confirmed", "rejected", "unresolved")
PROVENANCE_VALUES = ("provider-observed", "adapter-reported", "estimated",
                     "unavailable")
COVERAGE_VALUES = ("full", "partial", "none")
ISOLATION_EVIDENCE = ("verified", "unverified")
FINDING_FIELDS = ("id", "contract_or_path", "trigger", "consequence",
                  "evidence", "smallest_correction")
TIMING_FIELDS = ("packet_prep", "reviewer_startup", "review", "disposition",
                 "fixes", "implementation", "verification", "adjudication",
                 "total")


def _require(condition, message):
    if not condition:
        raise ValueError(message)


def validate_run_record(record):
    """Validate a run record; raise ValueError with a precise message."""
    _require(isinstance(record, dict), "record must be a JSON object")
    _require(type(record.get("schema_version")) is int
             and record["schema_version"] == SCHEMA_VERSION,
             "invalid schema_version")
    _require(record.get("experiment") == EXPERIMENT, "invalid experiment")
    _require(record.get("record_type") == "run", "invalid record_type")
    for key in ("case_id", "run_id"):
        _require(isinstance(record.get(key), str) and record[key].strip(),
                 "missing %s" % key)
    _require(record.get("arm") in ARMS, "invalid arm")
    _require(type(record.get("pair_index")) is int and record["pair_index"] >= 0,
             "invalid pair_index")
    _require(type(record.get("order_in_pair")) is int
             and record["order_in_pair"] in (0, 1), "invalid order_in_pair")

    review = record.get("review")
    _require(isinstance(review, dict), "missing review")
    _require(review.get("outcome") in OUTCOMES, "invalid review outcome")
    _require(isinstance(review.get("packet_hash"), str)
             and review["packet_hash"].startswith("sha256:"),
             "invalid packet_hash")
    for finding in review.get("findings", []):
        _require(isinstance(finding, dict), "finding must be an object")
        for field in FINDING_FIELDS:
            _require(isinstance(finding.get(field), str) and finding[field].strip(),
                     "finding missing %s" % field)
    finding_ids = [f["id"] for f in review.get("findings", [])]
    _require(len(finding_ids) == len(set(finding_ids)), "duplicate finding ids")
    for disposition in review.get("dispositions", []):
        _require(isinstance(disposition, dict), "disposition must be an object")
        _require(disposition.get("id") in finding_ids,
                 "disposition references unknown finding")
        _require(disposition.get("status") in DISPOSITION_STATUSES,
                 "invalid disposition status")
        if disposition["status"] == "confirmed":
            _require(isinstance(disposition.get("defect_id"), str)
                     and disposition["defect_id"].strip(),
                     "confirmed disposition needs defect_id")
        if disposition.get("duplicate_of") is not None:
            _require(disposition["duplicate_of"] in finding_ids,
                     "duplicate_of references unknown finding")

    impl = record.get("implementation")
    _require(isinstance(impl, dict) and isinstance(impl.get("ran"), bool),
             "implementation.ran must be boolean")
    if impl["ran"]:
        for key in ("final_task_success", "regression_seen"):
            _require(isinstance(impl.get(key), bool), "implementation.%s must be boolean" % key)
        _require(type(impl.get("repair_cycles")) is int
                 and impl["repair_cycles"] >= 0, "invalid repair_cycles")
        _require(type(impl.get("verification_cost")) is int
                 and impl["verification_cost"] >= 0, "invalid verification_cost")
        _require(isinstance(impl.get("escaped_defects"), list),
                 "escaped_defects must be a list")

    timing = record.get("timing_ms")
    _require(isinstance(timing, dict) and "total" in timing, "missing timing_ms.total")
    for key, value in timing.items():
        _require(key in TIMING_FIELDS, "unknown timing field %r" % key)
        _require(type(value) is int and value >= 0, "invalid timing %s" % key)

    usage = record.get("usage")
    _require(isinstance(usage, dict), "missing usage")
    _require(usage.get("provenance") in PROVENANCE_VALUES, "invalid usage provenance")
    _require(usage.get("coverage") in COVERAGE_VALUES, "invalid usage coverage")
    for key in ("token_cost", "context_bytes"):
        value = usage.get(key)
        _require(value is None or (type(value) is int and value >= 0),
                 "invalid %s" % key)
    value = usage.get("currency_cost")
    _require(value is None or (type(value) in (int, float) and value >= 0),
             "invalid currency_cost")

    ident = record.get("identities")
    _require(isinstance(ident, dict), "missing identities")
    _require(isinstance(ident.get("repo_commit"), str) and ident["repo_commit"].strip(),
             "missing repo_commit")
    _require(isinstance(ident.get("adapter"), str) and ident["adapter"].strip(),
             "missing adapter identity")
    _require(ident.get("isolation_evidence") in ISOLATION_EVIDENCE,
             "invalid isolation_evidence")
    return record
