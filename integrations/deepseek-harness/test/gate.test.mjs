// Unit tests for the strict completion gate: steering, loop safety, disposal.

import assert from 'node:assert/strict'
import test from 'node:test'

import { installCompletionGate } from '../lib/gate.mjs'
import { createFakeCtx } from './fake-ctx.mjs'

const SETTINGS = {
  strictCompletion: true,
  completionSignalSet: 'verification',
  maxContinuations: 3,
}

/** Builds a status stub returning scripted documents in order. */
function statusRunner(documents) {
  return {
    async status() {
      const next = documents.shift()
      if (next instanceof Error) {
        throw next
      }
      return next
    },
  }
}

/** Runs the registered turn-stopping listener with one agent. */
async function stop(ctx, agent) {
  await ctx.emit('agent/turn-stopping', { agent, turn: 1, signal: undefined })
}

test('green evidence permits the stop without steering', async () => {
  const ctx = createFakeCtx()
  installCompletionGate({
    ctx,
    settings: SETTINGS,
    runner: statusRunner([{ state: 'green', reason: 'current' }]),
    logger: ctx.logger,
  })
  const agent = { id: 'a', steered: [], steer(message) { this.steered.push(message) } }
  await stop(ctx, agent)
  assert.equal(agent.steered.length, 0)
})

test('stale, red, and missing steer with computational messages', async () => {
  const ctx = createFakeCtx()
  installCompletionGate({
    ctx,
    settings: SETTINGS,
    runner: statusRunner([
      { state: 'stale', reason: 'policy_changed', set: 'verification' },
      { state: 'red', reason: 'failures', set: 'verification', evidence: { failed: ['clippy', 'test'] } },
      { state: 'missing', reason: 'no_evidence', set: 'verification' },
    ]),
    logger: ctx.logger,
  })
  const agent = { id: 'a', steered: [], steer(message) { this.steered.push(message) } }
  await stop(ctx, agent)
  await stop(ctx, agent)
  await stop(ctx, agent)
  assert.equal(agent.steered.length, 3)
  assert.match(agent.steered[0].content[0].text, /do-harness\.toml changed/)
  assert.match(agent.steered[1].content[0].text, /clippy, test/)
  assert.match(agent.steered[2].content[0].text, /No current verification evidence/)
  for (const message of agent.steered) {
    assert.equal(message.role, 'user')
    // Session format v4 refuses the retired `plugin` wrapper.
    assert.equal(message.source.kind, 'do-harness')
  }
})

test('unchanged failures stop the loop after maxContinuations', async () => {
  const documents = Array.from({ length: 6 }, () => ({
    state: 'red',
    reason: 'failures',
    set: 'verification',
    evidence: { failed: ['clippy'] },
  }))
  const ctx = createFakeCtx()
  installCompletionGate({
    ctx,
    settings: { ...SETTINGS, maxContinuations: 3 },
    runner: statusRunner(documents),
    logger: ctx.logger,
  })
  const agent = { id: 'a', steered: [], steer(message) { this.steered.push(message) } }
  for (let i = 0; i < 6; i += 1) {
    await stop(ctx, agent)
  }
  assert.equal(agent.steered.length, 4, 'three continuations plus one exhaustion notice')
  assert.match(agent.steered[3].content[0].text, /blocked this turn/)
  assert.ok(
    ctx.warnings.some((warning) => warning.includes('blocked the turn')),
    'the blocked turn must be logged',
  )
})

test('alternating outcomes cannot extend the budget', async () => {
  const documents = [
    { state: 'red', reason: 'failures', set: 'verification', evidence: { failed: ['test'] } },
    { state: 'stale', reason: 'workspace_changed', set: 'verification' },
    { state: 'red', reason: 'failures', set: 'verification', evidence: { failed: ['clippy'] } },
    { state: 'stale', reason: 'policy_changed', set: 'verification' },
  ]
  const ctx = createFakeCtx()
  installCompletionGate({
    ctx,
    settings: { ...SETTINGS, maxContinuations: 2 },
    runner: statusRunner(documents),
    logger: ctx.logger,
  })
  const agent = { id: 'a', steered: [], steer(message) { this.steered.push(message) } }
  for (let i = 0; i < 4; i += 1) {
    await stop(ctx, agent)
  }
  assert.equal(agent.steered.length, 3, 'two continuations plus one exhaustion notice')
  assert.match(agent.steered[2].content[0].text, /blocked this turn after 3 attempts/)
})

test('an exhausted gate rejects the next step, which is the host blocked outcome', async () => {
  const ctx = createFakeCtx()
  installCompletionGate({
    ctx,
    settings: { ...SETTINGS, maxContinuations: 1 },
    runner: statusRunner([
      { state: 'missing', reason: 'no_evidence', set: 'verification' },
      { state: 'missing', reason: 'no_evidence', set: 'verification' },
    ]),
    logger: ctx.logger,
  })
  const agent = { id: 'a', steered: [], steer(message) { this.steered.push(message) } }
  await stop(ctx, agent)
  await stop(ctx, agent)
  assert.deepEqual(await ctx.waterfall('agent/pre-step', { agent }), { kind: 'reject' })
  assert.deepEqual(
    await ctx.waterfall('agent/pre-step', { agent }),
    { kind: 'enter' },
    'the block applies to one turn, not every later turn',
  )
})
test('running evidence steers as ordinary non-green, never as an infrastructure exception', async () => {
  const ctx = createFakeCtx()
  installCompletionGate({
    ctx,
    settings: { ...SETTINGS, maxContinuations: 1 },
    runner: statusRunner([
      { state: 'running', reason: 'inflight', set: 'verification', running: { pid: 7 } },
      { state: 'running', reason: 'inflight', set: 'verification', running: { pid: 7 } },
    ]),
    logger: ctx.logger,
  })
  const agent = { id: 'a', steered: [], steer(message) { this.steered.push(message) } }
  await stop(ctx, agent)
  await stop(ctx, agent)
  assert.equal(agent.steered.length, 2)
  assert.match(agent.steered[0].content[0].text, /currently running/)
  assert.match(agent.steered[0].content[0].text, /action="status"/)
  assert.match(agent.steered[1].content[0].text, /blocked this turn/)
  assert.ok(
    ctx.warnings.some((warning) => warning.includes('blocked the turn')),
    'the running state counts against the bounded continuation budget',
  )
})

test('infrastructure errors steer actionably and then stop', async () => {
  const ctx = createFakeCtx()
  installCompletionGate({
    ctx,
    settings: { ...SETTINGS, maxContinuations: 1 },
    runner: statusRunner([new Error('binary not found'), new Error('binary not found')]),
    logger: ctx.logger,
  })
  const agent = { id: 'a', steered: [], steer(message) { this.steered.push(message) } }
  await stop(ctx, agent)
  await stop(ctx, agent)
  assert.equal(agent.steered.length, 2)
  assert.match(agent.steered[0].content[0].text, /could not evaluate verification evidence/)
  assert.match(agent.steered[1].content[0].text, /binary not found/)
  assert.match(agent.steered[1].content[0].text, /blocked this turn/)
})

test('disabled strict mode registers no listener', () => {
  const ctx = createFakeCtx()
  const dispose = installCompletionGate({
    ctx,
    settings: { ...SETTINGS, strictCompletion: false },
    runner: statusRunner([]),
    logger: ctx.logger,
  })
  assert.equal(ctx.listeners.has('agent/turn-stopping'), false)
  assert.equal(dispose(), false)
})

test('the returned disposer removes the listener', async () => {
  const ctx = createFakeCtx()
  const dispose = installCompletionGate({
    ctx,
    settings: SETTINGS,
    runner: statusRunner([{ state: 'red', reason: 'failures', set: 'verification' }]),
    logger: ctx.logger,
  })
  const agent = { id: 'a', steered: [], steer(message) { this.steered.push(message) } }
  await stop(ctx, agent)
  assert.equal(agent.steered.length, 1)
  dispose()
  await stop(ctx, agent)
  assert.equal(agent.steered.length, 1, 'a disposed gate must not steer again')
  assert.equal(ctx.listeners.get('agent/pre-step').size, 0, 'the pre-step gate is disposed too')
})

test('distinct agents track loops independently', async () => {
  const ctx = createFakeCtx()
  installCompletionGate({
    ctx,
    settings: { ...SETTINGS, maxContinuations: 1 },
    runner: statusRunner([
      { state: 'red', reason: 'failures', set: 'verification' },
      { state: 'green', reason: 'current' },
    ]),
    logger: ctx.logger,
  })
  const first = { id: 'a', steered: [], steer(message) { this.steered.push(message) } }
  const second = { id: 'b', steered: [], steer(message) { this.steered.push(message) } }
  await stop(ctx, first)
  await stop(ctx, second)
  assert.equal(first.steered.length, 1, 'first agent gets its continuation')
  assert.equal(second.steered.length, 0, 'second agent starts with a fresh counter')
})
