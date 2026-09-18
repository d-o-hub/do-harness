#!/usr/bin/env bash
# Hermetic fake semantic provider for the PR-routing benchmark.
#
# Reads one routing request JSON object on stdin and emits the judgment named by
# FAKE_ROUTER_MODE. Deterministic by construction, so the benchmark is
# byte-identical across runs and never needs a network or a model.
#
# Modes:
#   docs          inert docs-shaped judgment (cheap)
#   api           public contract change (deep)
#   security      security-sensitive change (deep)
#   low-confidence answer set with confidence below the routing threshold
#   malformed     not JSON at all
#   bad-schema    unsupported schema_version
#   missing-fields required subfield absent
#   out-of-range  confidence outside [0,1]
#   timeout       sleeps past the wrapper's bound
#   misleading    superficially `docs`/cheap while asserting a public contract
#                 change: the atomic answer must still force `deep`
set -euo pipefail

# Consume the request and record its exact size when the benchmark asks for it,
# so router input bytes are measured rather than assumed.
payload="$(cat)"
if [[ -n "${FAKE_ROUTER_INPUT_BYTES:-}" ]]; then
  printf '%s' "$payload" | wc -c | tr -d ' ' > "$FAKE_ROUTER_INPUT_BYTES"
fi

case "${FAKE_ROUTER_MODE:?FAKE_ROUTER_MODE required}" in
  docs)
    printf '%s\n' '{"schema_version":1,"change_kind":"docs","behavior_change":{"answer":"no","confidence":0.95},"public_contract_change":{"answer":"no","confidence":0.98},"security_sensitive":{"answer":"no","confidence":0.99},"needs_repository_context":{"answer":"no","confidence":0.90},"confidence":0.95}'
    ;;
  tests)
    printf '%s\n' '{"schema_version":1,"change_kind":"tests","behavior_change":{"answer":"no","confidence":0.95},"public_contract_change":{"answer":"no","confidence":0.98},"security_sensitive":{"answer":"no","confidence":0.99},"needs_repository_context":{"answer":"no","confidence":0.90},"confidence":0.95}'
    ;;
  dependency)
    printf '%s\n' '{"schema_version":1,"change_kind":"dependency","behavior_change":{"answer":"no","confidence":0.95},"public_contract_change":{"answer":"no","confidence":0.98},"security_sensitive":{"answer":"no","confidence":0.99},"needs_repository_context":{"answer":"no","confidence":0.90},"confidence":0.95}'
    ;;
  refactor)
    printf '%s\n' '{"schema_version":1,"change_kind":"internal","behavior_change":{"answer":"no","confidence":0.95},"public_contract_change":{"answer":"no","confidence":0.98},"security_sensitive":{"answer":"no","confidence":0.99},"needs_repository_context":{"answer":"no","confidence":0.90},"confidence":0.95}'
    ;;
  behavior)
    printf '%s\n' '{"schema_version":1,"change_kind":"internal","behavior_change":{"answer":"yes","confidence":0.95},"public_contract_change":{"answer":"no","confidence":0.98},"security_sensitive":{"answer":"no","confidence":0.99},"needs_repository_context":{"answer":"no","confidence":0.90},"confidence":0.95}'
    ;;
  schema)
    printf '%s\n' '{"schema_version":1,"change_kind":"internal","behavior_change":{"answer":"yes","confidence":0.95},"public_contract_change":{"answer":"no","confidence":0.98},"security_sensitive":{"answer":"no","confidence":0.99},"needs_repository_context":{"answer":"yes","confidence":0.92},"confidence":0.95}'
    ;;
  concurrency)
    printf '%s\n' '{"schema_version":1,"change_kind":"internal","behavior_change":{"answer":"yes","confidence":0.95},"public_contract_change":{"answer":"no","confidence":0.98},"security_sensitive":{"answer":"yes","confidence":0.93},"needs_repository_context":{"answer":"yes","confidence":0.92},"confidence":0.95}'
    ;;
  api)
    printf '%s\n' '{"schema_version":1,"change_kind":"public-api","behavior_change":{"answer":"yes","confidence":0.95},"public_contract_change":{"answer":"yes","confidence":0.98},"security_sensitive":{"answer":"no","confidence":0.99},"needs_repository_context":{"answer":"no","confidence":0.90},"confidence":0.95}'
    ;;
  security)
    printf '%s\n' '{"schema_version":1,"change_kind":"security","behavior_change":{"answer":"yes","confidence":0.95},"public_contract_change":{"answer":"no","confidence":0.98},"security_sensitive":{"answer":"yes","confidence":0.99},"needs_repository_context":{"answer":"no","confidence":0.90},"confidence":0.95}'
    ;;
  low-confidence)
    printf '%s\n' '{"schema_version":1,"change_kind":"docs","behavior_change":{"answer":"no","confidence":0.40},"public_contract_change":{"answer":"no","confidence":0.40},"security_sensitive":{"answer":"no","confidence":0.40},"needs_repository_context":{"answer":"no","confidence":0.40},"confidence":0.40}'
    ;;
  malformed)
    printf 'provider is not json\n'
    ;;
  bad-schema)
    printf '%s\n' '{"schema_version":2,"change_kind":"docs","behavior_change":{"answer":"no","confidence":0.95},"public_contract_change":{"answer":"no","confidence":0.98},"security_sensitive":{"answer":"no","confidence":0.99},"needs_repository_context":{"answer":"no","confidence":0.90},"confidence":0.95}'
    ;;
  missing-fields)
    printf '%s\n' '{"schema_version":1,"change_kind":"docs","behavior_change":{"answer":"no","confidence":0.95},"confidence":0.95}'
    ;;
  out-of-range)
    printf '%s\n' '{"schema_version":1,"change_kind":"docs","behavior_change":{"answer":"no","confidence":1.5},"public_contract_change":{"answer":"no","confidence":0.98},"security_sensitive":{"answer":"no","confidence":0.99},"needs_repository_context":{"answer":"no","confidence":0.90},"confidence":0.95}'
    ;;
  timeout)
    sleep 30
    printf '%s\n' '{"schema_version":1,"change_kind":"docs","behavior_change":{"answer":"no","confidence":0.95},"public_contract_change":{"answer":"no","confidence":0.98},"security_sensitive":{"answer":"no","confidence":0.99},"needs_repository_context":{"answer":"no","confidence":0.90},"confidence":0.95}'
    ;;
  misleading)
    # Look at the shape, not the label: change_kind says docs, but the injected
    # unit is a public contract change and the atomic answer must win.
    printf '%s\n' '{"schema_version":1,"change_kind":"docs","behavior_change":{"answer":"no","confidence":0.99},"public_contract_change":{"answer":"yes","confidence":0.99},"security_sensitive":{"answer":"no","confidence":0.99},"needs_repository_context":{"answer":"no","confidence":0.99},"confidence":0.99}'
    ;;
  *)
    printf 'unknown FAKE_ROUTER_MODE: %s\n' "$FAKE_ROUTER_MODE" >&2
    exit 1
    ;;
esac
