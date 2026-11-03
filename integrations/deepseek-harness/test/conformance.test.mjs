// Offline unit tests for the completion-integration result contract.
//
// These are labeled separately from real-host results: they prove the contract
// classifies observations and refuses malformed documents. The real-host runner
// (`conformance/run.mjs`) is the only source of host evidence.

import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'

import {
  buildResult,
  classifyScenario,
  deriveTerminalResult,
  deriveVerdict,
  REASON_CODES,
  SCENARIOS,
  validateResult,
} from '../lib/conformance.mjs'
import { adapterDigest, readAdapterIdentity } from '../conformance/host.mjs'

const CLIENT = { id: 'deepseek-harness', version: '0.1.7-rc.2' }
const ADAPTER = { name: 'dsh-do-harness', version: '0.1.0', contentDigest: `sha256:${'a'.repeat(64)}` }
const HARNESS = { version: '0.4.2', evidenceSchema: 5 }

/** Observations for a host whose gate works and whose evidence ends green. */
function passingObservations() {
  return SCENARIOS.map((scenario) => ({
    id: scenario.id,
    terminal: expectedObserved(scenario.id),
    continuations: scenario.expectedTerminal === 'blocked' ? 2 : 0,
    evidenceGreenAtClose: scenario.expectedTerminal === 'verified',
    error: null,
  }))
}

function expectedObserved(id) {
  switch (id) {
    case 'fresh-pass':
    case 'repair-and-reverify':
      return 'verified'
    case 'plugin-load-failure':
      return 'unsupported'
    case 'user-cancellation':
      return 'cancelled'
    default:
      return 'blocked'
  }
}

function build(observations) {
  return buildResult({ client: CLIENT, adapter: ADAPTER, harness: HARNESS, observations })
}

test('a fully observed lifecycle passes and validates', () => {
  const result = build(passingObservations())
  assert.equal(result.verdict, 'pass')
  assert.equal(result.terminal_result, 'verified')
  assert.equal(result.scenarios.length, SCENARIOS.length)
  assert.doesNotThrow(() => validateResult(result))
})

test('completion accepted while evidence is not green fails', () => {
  const observations = passingObservations().map((entry) =>
    entry.id === 'missing-or-malformed-evidence'
      ? { ...entry, terminal: 'verified', evidenceGreenAtClose: false }
      : entry,
  )
  const result = build(observations)
  const entry = result.scenarios.find((scenario) => scenario.id === 'missing-or-malformed-evidence')
  assert.equal(entry.outcome, 'fail')
  assert.equal(entry.reason, 'blocked-outcome-absent')
  assert.equal(result.verdict, 'fail')
  assert.equal(result.terminal_result, 'blocked')
})

test('the gate being absent is detected: removal of the stop integration fails', () => {
  // Negative control: with no stop hook the host closes every turn immediately,
  // so the stale-evidence scenario can never withhold completion.
  const observations = passingObservations().map((entry) => ({
    ...entry,
    terminal: entry.id === 'plugin-load-failure' ? 'unsupported' : 'verified',
    continuations: 0,
    evidenceGreenAtClose: entry.id === 'fresh-pass' || entry.id === 'repair-and-reverify',
  }))
  const result = build(observations)
  assert.equal(result.verdict, 'fail')
  assert.equal(result.scenarios.find((entry) => entry.id === 'green-then-edit').reason, 'blocked-outcome-absent')
})

test('driver infrastructure failures stay unverified, not accepted', () => {
  const result = build([
    {
      id: 'infrastructure-error',
      terminal: 'verified',
      continuations: 2,
      evidenceGreenAtClose: false,
      error: null,
      infrastructure: true,
    },
  ])
  const entry = result.scenarios.find((scenario) => scenario.id === 'infrastructure-error')
  assert.equal(entry.outcome, 'fail')
  assert.equal(entry.reason, 'infrastructure-error')
  // Scenarios with no observation remain unknown, never green.
  assert.equal(result.verdict, 'fail')
})

test('an unexercised scenario is unsupported, not a pass', () => {
  const result = build([])
  assert.equal(result.verdict, 'unknown')
  assert.equal(result.terminal_result, 'unsupported')
  assert.ok(result.scenarios.every((entry) => entry.outcome === 'unknown'))
  assert.ok(result.scenarios.every((entry) => entry.reason === 'not-exercised'))
})

test('classifyScenario ignores adapter intent and grades the host', () => {
  const entry = classifyScenario({
    id: 'failing-sensor',
    expectedTerminal: 'blocked',
    observed: {
      terminal: 'verified',
      continuations: 3,
      evidenceGreenAtClose: false,
      error: null,
    },
  })
  assert.equal(entry.outcome, 'fail')
  assert.equal(entry.observed_terminal, 'verified')
})

test('terminal result and verdict are derived, not supplied', () => {
  assert.equal(deriveTerminalResult([{ outcome: 'pass' }]), 'verified')
  assert.equal(deriveTerminalResult([{ outcome: 'unknown' }]), 'unsupported')
  assert.equal(deriveTerminalResult([{ outcome: 'fail' }, { outcome: 'pass' }]), 'blocked')
  assert.equal(deriveVerdict([{ outcome: 'pass' }]), 'pass')
  assert.equal(deriveVerdict([{ outcome: 'fail' }]), 'fail')
  assert.equal(deriveVerdict([{ outcome: 'unknown' }]), 'unknown')
})

test('validateResult refuses documents that break the contract', () => {
  const base = build(passingObservations())
  const badReason = structuredClone(base)
  badReason.scenarios[0].reason = 'looks-fine-to-me'
  assert.throws(() => validateResult(badReason), /unfixed reason code/)

  const duplicate = structuredClone(base)
  duplicate.scenarios[1].id = duplicate.scenarios[0].id
  assert.throws(() => validateResult(duplicate), /duplicate scenario id|missing scenario/)

  const lyingVerdict = structuredClone(base)
  lyingVerdict.scenarios[0].outcome = 'fail'
  assert.throws(() => validateResult(lyingVerdict), /disagrees/)

  const badDigest = structuredClone(base)
  badDigest.adapter.content_digest = 'sha256:not-a-digest'
  assert.throws(() => validateResult(badDigest), /content_digest/)
})

test('the schema and the module agree on the scenario set and reason codes', () => {
  const schema = JSON.parse(
    readFileSync(new URL('../../../schemas/completion-integration-v1.schema.json', import.meta.url), 'utf8'),
  )
  const scenarioEnum = schema.properties.scenarios.items.properties.id.enum
  assert.deepEqual(scenarioEnum, SCENARIOS.map((scenario) => scenario.id))
  const reasonEnum = schema.properties.scenarios.items.properties.reason.enum
  assert.deepEqual(reasonEnum, [...REASON_CODES])
})

test('the adapter digest names exact content', () => {
  const bundle = new URL('..', import.meta.url).pathname
  const digest = adapterDigest(bundle)
  assert.match(digest, /^sha256:[0-9a-f]{64}$/)
  assert.equal(digest, adapterDigest(bundle), 'the digest must be deterministic')
  const identity = readAdapterIdentity(bundle)
  assert.equal(identity.name, 'dsh-do-harness')
})
