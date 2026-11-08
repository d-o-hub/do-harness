// Versioned completion-integration result contract.
//
// The artifact records what one version-pinned client actually did at the stop
// boundary: the exact client and adapter identity, the harness version and
// evidence schema, one entry per required lifecycle scenario, and a terminal
// result. It is deliberately separate from the client-boundary (privacy)
// conformance result: sanitization evidence says nothing about completion.

export const SCHEMA_VERSION = 1
export const CLAIM = 'completion-gating'
export const TEST_SUITE_VERSION = '1'

export const TERMINAL_RESULTS = ['verified', 'blocked', 'cancelled', 'unsupported']
export const OUTCOMES = ['pass', 'fail', 'unknown']

/** Scenario order is part of the contract: reporters render it as-is. */
export const SCENARIOS = Object.freeze([
  { id: 'fresh-pass', expectedTerminal: 'verified' },
  { id: 'green-then-edit', expectedTerminal: 'blocked' },
  { id: 'repair-and-reverify', expectedTerminal: 'verified' },
  { id: 'failing-sensor', expectedTerminal: 'blocked' },
  { id: 'missing-or-malformed-evidence', expectedTerminal: 'blocked' },
  { id: 'infrastructure-error', expectedTerminal: 'blocked' },
  { id: 'retry-exhaustion', expectedTerminal: 'blocked' },
  { id: 'plugin-load-failure', expectedTerminal: 'unsupported' },
  { id: 'user-cancellation', expectedTerminal: 'cancelled' },
  { id: 'edit-explain-run-same-scope', expectedTerminal: 'verified' },
  { id: 'eligible-reuse', expectedTerminal: 'verified' },
  { id: 'missing-dependency-forces-execution', expectedTerminal: 'blocked' },
  { id: 'bounded-resume-context', expectedTerminal: 'verified' },
  { id: 'strict-completion-then-status', expectedTerminal: 'verified' },
])

export const REASON_CODES = Object.freeze([
  'completion-accepted',
  'completion-withheld',
  'blocked-outcome-absent',
  'continuation-exhausted',
  'stale-evidence',
  'red-evidence',
  'missing-evidence',
  'malformed-evidence',
  'infrastructure-error',
  'plugin-load-failure',
  'stop-integration-missing',
  'user-cancelled',
  'unsupported-host',
  'adapter-failed',
  'timeout',
  'not-exercised',
])

export const COVERAGE_LIMITATIONS = Object.freeze([
  'One named client version and one adapter content digest; other hosts, versions, or adapter edits are unverified.',
  'Turns are synthetic and the model is a scripted stub: the harness proves host lifecycle, not model judgement.',
  'Only the observed stop boundary and terminal outcome are claimed; prompts, tool arguments, and child streams are never recorded.',
  'A client that changes its lifecycle events invalidates these observations until the suite is rerun against the new version.',
])

/**
 * Classifies one scenario from what the host did, never from what the adapter
 * intended. A completion accepted while evidence was not green is a failure,
 * even though the host reported success: that is exactly the distinction the
 * claim requires.
 */
export function classifyScenario({ id, expectedTerminal, observed }) {
  const base = {
    id,
    expected_terminal: expectedTerminal,
    observed_terminal: observed.terminal,
    continuations: observed.continuations,
    evidence_green_at_close: observed.evidenceGreenAtClose,
  }
  if (observed.exercised === false) {
    return { ...base, outcome: 'unknown', reason: 'not-exercised' }
  }
  if (observed.error !== null && observed.error !== undefined) {
    return {
      ...base,
      outcome: 'fail',
      reason: observed.error === 'timeout' ? 'timeout' : 'adapter-failed',
    }
  }
  if (observed.terminal === 'unsupported') {
    return {
      ...base,
      outcome: expectedTerminal === 'unsupported' ? 'pass' : 'fail',
      reason: expectedTerminal === 'unsupported' ? 'plugin-load-failure' : 'unsupported-host',
    }
  }
  if (observed.terminal === 'cancelled') {
    return {
      ...base,
      outcome: expectedTerminal === 'cancelled' ? 'pass' : 'fail',
      reason: 'user-cancelled',
    }
  }
  if (observed.terminal === 'verified') {
    if (!observed.evidenceGreenAtClose) {
      return {
        ...base,
        outcome: 'fail',
        reason: observed.infrastructure ? 'infrastructure-error' : 'blocked-outcome-absent',
      }
    }
    return {
      ...base,
      outcome: expectedTerminal === 'verified' ? 'pass' : 'fail',
      reason: observed.continuations > 0 ? 'completion-withheld' : 'completion-accepted',
    }
  }
  return {
    ...base,
    outcome: expectedTerminal === 'blocked' ? 'pass' : 'fail',
    reason: observed.reason ?? 'completion-withheld',
  }
}

/** Derives the terminal result from the scenario outcomes. */
export function deriveTerminalResult(entries) {
  if (entries.some((entry) => entry.outcome === 'fail')) {
    return 'blocked'
  }
  if (entries.some((entry) => entry.outcome === 'unknown')) {
    return 'unsupported'
  }
  return 'verified'
}

/** Derives the claim verdict from the scenario outcomes. */
export function deriveVerdict(entries) {
  if (entries.some((entry) => entry.outcome === 'fail')) {
    return 'fail'
  }
  if (entries.some((entry) => entry.outcome === 'unknown')) {
    return 'unknown'
  }
  return 'pass'
}

/** Builds the result document in the canonical field order. */
export function buildResult({
  client,
  adapter,
  harness,
  observations,
  timestamp = new Date().toISOString(),
  limitations = COVERAGE_LIMITATIONS,
}) {
  const byId = new Map(observations.map((observation) => [observation.id, observation]))
  const scenarios = SCENARIOS.map((scenario) =>
    classifyScenario({
      id: scenario.id,
      expectedTerminal: scenario.expectedTerminal,
      observed: byId.get(scenario.id) ?? notExercised(),
    }),
  )
  return {
    schema_version: SCHEMA_VERSION,
    claim: CLAIM,
    test_suite_version: TEST_SUITE_VERSION,
    client: { id: client.id, version: client.version },
    adapter: {
      name: adapter.name,
      version: adapter.version,
      content_digest: adapter.contentDigest,
    },
    harness: { version: harness.version, evidence_schema: harness.evidenceSchema },
    environment: { node: process.version, turns: 'scripted', model: 'synthetic' },
    scenarios,
    terminal_result: deriveTerminalResult(scenarios),
    verdict: deriveVerdict(scenarios),
    coverage_limitations: [...limitations],
    timestamp,
  }
}

function notExercised() {
  return {
    terminal: 'unsupported',
    continuations: 0,
    evidenceGreenAtClose: false,
    error: null,
    exercised: false,
  }
}

/** Validates the document against the contract; throws on the first defect. */
export function validateResult(result) {
  requireObject(result, 'result')
  requireEqual(result.schema_version, SCHEMA_VERSION, 'schema_version')
  requireEqual(result.claim, CLAIM, 'claim')
  requireEqual(result.test_suite_version, TEST_SUITE_VERSION, 'test_suite_version')
  for (const field of ['client', 'adapter', 'harness', 'environment']) {
    requireObject(result[field], field)
  }
  if (!Array.isArray(result.scenarios) || result.scenarios.length !== SCENARIOS.length) {
    throw new Error(`completion result must carry ${SCENARIOS.length} scenarios`)
  }
  const seen = new Set()
  for (const entry of result.scenarios) {
    requireObject(entry, 'scenario')
    if (seen.has(entry.id)) {
      throw new Error(`duplicate scenario id: ${entry.id}`)
    }
    seen.add(entry.id)
    if (!TERMINAL_RESULTS.includes(entry.observed_terminal)) {
      throw new Error(`scenario ${entry.id} has an unknown observed terminal`)
    }
    if (!OUTCOMES.includes(entry.outcome)) {
      throw new Error(`scenario ${entry.id} has an unknown outcome`)
    }
    if (!REASON_CODES.includes(entry.reason)) {
      throw new Error(`scenario ${entry.id} has an unfixed reason code: ${entry.reason}`)
    }
  }
  for (const scenario of SCENARIOS) {
    if (!seen.has(scenario.id)) {
      throw new Error(`missing scenario: ${scenario.id}`)
    }
  }
  if (!TERMINAL_RESULTS.includes(result.terminal_result)) {
    throw new Error('terminal_result is not a fixed outcome')
  }
  if (result.terminal_result !== deriveTerminalResult(result.scenarios)) {
    throw new Error('terminal_result disagrees with the scenario outcomes')
  }
  if (result.verdict !== deriveVerdict(result.scenarios)) {
    throw new Error('verdict disagrees with the scenario outcomes')
  }
  if (!Array.isArray(result.coverage_limitations) || result.coverage_limitations.length === 0) {
    throw new Error('coverage_limitations must be nonempty')
  }
  requireDigest(result.adapter.content_digest)
  return result
}

function requireObject(value, field) {
  if (value === null || typeof value !== 'object' || Array.isArray(value)) {
    throw new Error(`${field} must be an object`)
  }
}

function requireEqual(actual, expected, field) {
  if (actual !== expected) {
    throw new Error(`${field} must be ${JSON.stringify(expected)}`)
  }
}

function requireDigest(value) {
  if (typeof value !== 'string' || !/^sha256:[0-9a-f]{64}$/.test(value)) {
    throw new Error('adapter.content_digest must be sha256:<64 lowercase hex>')
  }
}
