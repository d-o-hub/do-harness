# Client boundary conformance

| Claim | Evidence | Limit |
| --- | --- | --- |
| Guidance installed | Pinned skill manifest and drift check | Says nothing about execution |
| Sanitizer fixture passed | Optional context-sanitization sensor | Tests one sanitizer fixture |
| Client boundary verified | Client-conformance protocol v1 | Tests only named, instrumented routes |

`init` copies the dependency-free `scripts/check-client-conformance.py` helper
but enables no sensor. An unconfigured or unsupported client produces `unknown`
and a `SKIP:` marker. `--required` fails closed on unknown. A fixture-only
result is malformed client evidence and cannot satisfy this gate.

Explicitly add a `client-conformance` sensor invoking the helper with
`--required`, `--client`, `--client-version`, `--integration`, and finally
`--adapter` followed by the trusted adapter's argv. Add it to the required
signal set. Allow at least 45 seconds for the four ten-second cases.
The helper prints one JSON result; advisory unknown adds a `SKIP:` line.
The result follows [client-conformance v1](../schemas/client-conformance-v1.schema.json).
CI must run the helper for each gate, rather than trust a replayed result.

## Adapter contract

The helper never imports a sanitizer or opens a network connection. For each
of `prompt`, `tool-arguments`, `file-excerpt`, and `outbound-request`, it sends
one JSON request on stdin: `protocol_version` (1), `scenario_id`,
`expected_policy` (`transform-or-refuse`), `private_markers` (fresh synthetic
strings), and `payload` (a map containing synthetic context). The adapter
drives that specific route through the named client. It must replace the real
outbound transport with a local fake boundary and observe bytes there, after
mediation, rather than call a sanitizer directly.

Return exactly `protocol_version` (1), `supported` (boolean), `instrumented`
(boolean), `refused` (boolean), and `outbound` (the observed synthetic bytes).
A refusal passes only if no bytes crossed the boundary. A transformation
passes only with a nonempty observed payload and no raw private marker in
either output stream. Missing instrumentation or observation is unknown.
Malformed observations and leakage fail. Both streams are discarded; only
fixed reason codes are emitted. Reports contain no raw synthetic markers.

## Examples and limits

The [local fake-client adapter](../tests/fixtures/client-conformance/adapter.py)
instruments a fake transport, drives each entrypoint, and captures transformed
bytes. It demonstrates the supported test-harness integration, with no network
or API key. It makes no claim about a real coding client. For an unsupported
client without replaceable transport, return `supported: false`,
`instrumented: false`, `refused: false`, and an empty `outbound`. All scenarios
are unknown and required policy fails. Tests cover both examples and refusal,
leakage, malformed results, and absent instrumentation.

Adapters are trusted executable test code, not a sandbox. A dishonest adapter
can fabricate observations. Review and pin it and the client version; results
are observations under that adapter, not attestations or universal mediation.
Alternate routes, plugins, updates, and production traffic remain unverified.
A transformation proves absence of raw markers, not resistance to encoding,
side channels, or all PII. Never use production data, credentials or vault
mappings. Live integration runs must be explicit and use a local fake endpoint
by default. Ordinary tests use subprocess test harnesses without network.
