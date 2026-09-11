// Unit tests for plugin loading, binary probing, and configuration.

import assert from 'node:assert/strict'
import test from 'node:test'

import { HarnessRunner } from '../lib/harness.mjs'
import { createFakeCtx } from './fake-ctx.mjs'

const { apply, normalizeConfig } = await import('../index.js')

test('normalizeConfig applies documented defaults', () => {
  const settings = normalizeConfig({ workspace: '/repo' })
  assert.equal(settings.defaultSignalSet, 'feedback')
  assert.equal(settings.completionSignalSet, 'verification')
  assert.equal(settings.strictCompletion, false)
  assert.equal(settings.maxContinuations, 3)
  assert.equal(settings.binary, undefined)
})

test('normalizeConfig rejects malformed values', () => {
  assert.throws(() => normalizeConfig(null), /config must be an object/)
  assert.throws(() => normalizeConfig({ workspace: 'relative/path' }), /absolute path/)
  assert.throws(() => normalizeConfig({ strictCompletion: 'yes' }), /must be a boolean/)
  assert.throws(() => normalizeConfig({ maxContinuations: 0 }), /positive integer/)
})

test('apply probes the binary and registers the tool', async () => {
  const ctx = createFakeCtx({
    responses: [{ exitCode: 0, stdout: '{"name":"do-harness"}' }],
  })
  await apply(ctx, { workspace: '/repo', binary: 'do-harness' })
  assert.ok(ctx.tool('development_signals'), 'tool must be registered')
  assert.equal(ctx.spawnedArgv[0].join(' '), 'do-harness version --format json')
  // strictCompletion defaults off: no turn-stopping listener.
  assert.equal(ctx.listeners.has('agent/turn-stopping'), false)
})

test('apply fails loudly when the binary cannot start', async () => {
  const ctx = createFakeCtx({ responses: [] })
  ctx.subprocess.spawn = () => {
    throw new Error('ENOENT')
  }
  await assert.rejects(
    () => apply(ctx, { workspace: '/repo', binary: 'do-harness' }),
    /cannot execute `do-harness`.*DO_HARNESS_BIN/s,
  )
})

test('apply fails loudly when the probe exits non-zero', async () => {
  const ctx = createFakeCtx({ responses: [{ exitCode: 1, stderr: 'boom' }] })
  await assert.rejects(() => apply(ctx, { workspace: '/repo', binary: 'do-harness' }), /not runnable/)
})

test('strictCompletion registers the gate and gates on cheap status only', async () => {
  const ctx = createFakeCtx({
    responses: [
      { exitCode: 0, stdout: '{}' }, // probe
      { exitCode: 1, stdout: JSON.stringify({ state: 'stale', reason: 'workspace_changed', set: 'verification' }) },
    ],
  })
  await apply(ctx, { workspace: '/repo', binary: 'do-harness', strictCompletion: true })
  const agent = {
    id: 'a1',
    steered: [],
    steer(message) {
      this.steered.push(message)
    },
  }
  await ctx.emit('agent/turn-stopping', { agent, turn: 1, signal: undefined })
  assert.equal(agent.steered.length, 1)
  assert.match(agent.steered[0].content[0].text, /stale/)
  assert.match(agent.steered[0].content[0].text, /workspace changed/)
  assert.equal(agent.steered[0].source.kind, 'plugin')
  // The gate must not execute sensors: only the probe + status were spawned.
  assert.equal(ctx.spawnedArgv.length, 2)
})

test('runner exposes status/explain without sensor execution', async () => {
  const ctx = createFakeCtx({
    responses: [
      { exitCode: 1, stdout: JSON.stringify({ state: 'missing', reason: 'no_evidence' }) },
      { exitCode: 0, stdout: JSON.stringify({ set: 'verification', changed_files: [], selected: [], skipped: [] }) },
    ],
  })
  const runner = new HarnessRunner({ ctx, binary: 'do-harness', workspace: '/tmp' })
  const status = await runner.status({ set: 'verification' })
  assert.equal(status.state, 'missing')
  const explain = await runner.explain({ set: 'verification', changed: false })
  assert.equal(explain.set, 'verification')
})
