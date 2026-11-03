# Client capability profile

`do-harness doctor --client-profile <file>` reports what a coding client was
*observed* to enforce, one claim at a time. It is opt-in: without the flag,
`doctor` output is unchanged.

The profile declares expectations. Evidence comes from a recorded observation
document, a [client-conformance](client-conformance.md) report, or an
explicitly invoked adapter. A declaration never satisfies a claim, and the
claims are never collapsed into a single "safe" boolean.

| Claim | Evidence | Limit |
| --- | --- | --- |
| `guidance_installed` | Installed guidance marker | Says nothing about execution |
| `host_boundary` | Permission-denial observation on a named route | Only the tested route |
| `mcp_mediation` | Named MCP/tool-call route observation | Local shell and file tools are not covered |
| `governance_enforced` | Proxy governance mode on the tested route | Other traffic bypasses the proxy |
| `completion_gated` | Completion-path observation | Only the tested completion path |
| `context_sanitization` | `sanitizer_fixture` from a conformance report | One sanitizer fixture |
| `outbound_mediation` | `client_boundary` from a conformance report | Named instrumented routes only |

## Profile

The document is described by
[`schemas/client-profile-v1.schema.json`](../schemas/client-profile-v1.schema.json).

<!-- example: id=client-profile classification=parser-check-only reason="Bounded profile document; parsed by the client-profile unit tests." -->
```json
{
  "schema_version": 1,
  "client": { "id": "acme-client", "version": "1.4.0" },
  "configuration": "default-config",
  "adapter": { "argv": ["python3", "adapter.py"], "sha256": "<64 lowercase hex characters>" },
  "observations": "observations.json",
  "conformance_report": "conformance.json",
  "claims": {
    "guidance_installed": { "required": true },
    "host_boundary": { "required": true, "route": "shell" },
    "mcp_mediation": { "required": true, "route": "mcp-tool-call" },
    "governance_enforced": { "required": true, "route": "mcp-tool-call" },
    "completion_gated": { "required": false, "route": "mcp-tool-call" },
    "context_sanitization": { "required": false }
  }
}
```

Paths are resolved relative to the profile file. `adapter.sha256` pins the
trusted local adapter; `--client-probe` refuses to run when the executable's
digest differs. `host_boundary`, `mcp_mediation`, `governance_enforced`, and
`completion_gated` must name the route they were tested on.

## Observations

The document is described by
[`schemas/client-observation-v1.schema.json`](../schemas/client-observation-v1.schema.json).

<!-- example: id=client-observation classification=parser-check-only reason="Bounded observation document; parsed by the client-profile unit tests." -->
```json
{
  "schema_version": 1,
  "client": { "id": "acme-client", "version": "1.4.0" },
  "adapter": { "sha256": "<64 lowercase hex characters>" },
  "configuration": "default-config",
  "generated_at_unix_seconds": 1767225600,
  "max_age_seconds": 3600,
  "governance": { "mode": "enforced", "route": "mcp-tool-call" },
  "claims": {
    "host_boundary": { "route": "shell", "status": "pass", "reason": "observed" },
    "mcp_mediation": { "route": "mcp-tool-call", "status": "pass", "reason": "observed" },
    "governance_enforced": { "route": "mcp-tool-call", "status": "pass", "reason": "observed" }
  }
}
```

An observation may claim only `pass`, `fail`, or `unknown`. `stub`,
`degraded`, `stale`, `unsupported`, and `invalidated` are derived by
`do-harness` and cannot be asserted by a document. Reason codes are fixed, and
free text is rejected rather than rendered.

An observation is bound to the pinned client id and version, adapter digest,
configuration, and per-claim route. A changed pin, a document older than
`max_age_seconds`, or a timestamp in the future invalidates it. A default
proxy build reports `stub` and cannot satisfy a required `governance_enforced`
claim; a degraded or unreachable proxy reports `degraded`.

## Probe mode

`--client-probe` runs the pinned adapter once, in a disposable temporary
workspace, with a five-second timeout and 64 KiB per stream. The request
carries a fresh synthetic marker; a marker echoed on either stream is a
dishonest observation. Nothing runs without the flag, and no network request,
model call, or mutation happens implicitly.

## Strict diagnostics

`--strict` fails the run when a required claim is not `pass` — including
`fail`, `stub`, `degraded`, `stale`, `unsupported`, `invalidated`, or
`unknown`. Advisory claims are reported and never fail the run.

## Limits

Adapters are trusted local executables, not a sandbox: a dishonest adapter can
fabricate observations. Results are observations under a pinned adapter, not
attestations. Native sandboxing stays in the client or outer runner; this
report never claims universal containment. Reports contain fixed reason codes
and limitation text only — no adapter output, tokens, environment dumps, raw
prompts, or private paths.
