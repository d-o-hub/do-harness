// Unit tests for the model-facing development_signals tool.

import assert from 'node:assert/strict'
import test from 'node:test'

import { HarnessRunner } from '../lib/harness.mjs'
import { defineDevelopmentSignalsTool, validateArgs } from '../lib/tool.mjs'
import { createFakeCtx } from './fake-ctx.mjs'

const SETTINGS = {
  defaultSignalSet: 'feedback',
  completionSignalSet: 'verification',
}

/** Builds a tool wired to a fake ctx with canned subprocess responses. */
function toolWith(responses) {
  const ctx = createFakeCtx({ responses })
  const runner = new HarnessRunner({ ctx, binary: 'do-harness', workspace: '/tmp' })
  return { ctx, tool: defineDevelopmentSignalsTool({ runner, settings: SETTINGS }) }
}

test('validateArgs accepts the documented shape and rejects junk', () => {
  assert.deepEqual(validateArgs({ action: 'run', set: ' feedback ', changed: true }), {
    action: 'run',
    set: 'feedback',
    changed: true,
  })
  assert.deepEqual(validateArgs({ action: 'status' }), {
    action: 'status',
    set: undefined,
    changed: false,
  })
  assert.throws(() => validateArgs(null), /must be an object/)
  assert.throws(() => validateArgs({ action: 'exec' }), /action must be/)
  assert.throws(() => validateArgs({ action: 'run', set: '' }), /set must be/)
  assert.throws(() => validateArgs({ action: 'run', changed: 'yes' }), /changed must be/)
})

test('execute maps actions to fixed argv and defaults', async () => {
  const { ctx, tool } = toolWith([
    {
      exitCode: 0,
      stdout: JSON.stringify({
        ok: true,
        signal_set: 'feedback',
        failed: [],
        sensors: [{ name: 'fmt', ok: true, duration_ms: 4 }],
      }),
    },
    { exitCode: 0, stdout: JSON.stringify({ state: 'green', reason: 'current' }) },
    {
      exitCode: 0,
      stdout: JSON.stringify({ set: 'verification', changed_files: [], selected: [], skipped: [] }),
    },
  ])

  const run = await tool.execute({ action: 'run' }, { signal: undefined })
  assert.equal(run.state, 'green')
  assert.equal(run.vacuous, false)
  const status = await tool.execute({ action: 'status' }, { signal: undefined })
  assert.equal(status.state, 'green')
  await tool.execute({ action: 'explain', changed: true }, { signal: undefined })

  const argvs = ctx.spawnedArgv.map((argv) => argv.join(' '))
  assert.ok(argvs[0].includes('verify --set feedback --format json'))
  assert.ok(argvs[1].includes('status --set verification --format json'))
  assert.ok(argvs[2].includes('explain --set verification --format json --changed'))
})

test('a zero-sensor run is green but explicitly vacuous', async () => {
  const { tool } = toolWith([
    {
      exitCode: 0,
      stdout: JSON.stringify({ ok: true, signal_set: 'feedback', failed: [], sensors: [] }),
    },
  ])
  const value = await tool.execute({ action: 'run' }, { signal: undefined })
  assert.equal(value.state, 'green')
  assert.equal(value.vacuous, true)
  assert.deepEqual(value.passed, [])
})

test('a non-zero verify is a red domain state, not an exception', async () => {
  const { tool } = toolWith([
    {
      exitCode: 1,
      stdout: JSON.stringify({
        ok: false,
        signal_set: 'feedback',
        failed: ['clippy'],
        sensors: [
          { name: 'fmt', ok: true, duration_ms: 1 },
          { name: 'clippy', ok: false, duration_ms: 2 },
        ],
      }),
    },
  ])
  const value = await tool.execute({ action: 'run', set: 'feedback' }, { signal: undefined })
  assert.equal(value.state, 'red')
  assert.deepEqual(value.failed, ['clippy'])
  assert.deepEqual(value.passed, ['fmt'])
  assert.equal(value.duration_ms, 3)
})

test('malformed JSON and exit 2 surface actionable errors', async () => {
  const { tool } = toolWith([
    { exitCode: 0, stdout: 'not json' },
    { exitCode: 2, stderr: 'unknown signal set release' },
  ])
  await assert.rejects(
    () => tool.execute({ action: 'run', set: 'feedback' }, { signal: undefined }),
    /did not return JSON/,
  )
  await assert.rejects(
    () => tool.execute({ action: 'status', set: 'feedback' }, { signal: undefined }),
    /rejected the request/,
  )
})

test('status documents with an unrecognized state are rejected', async () => {
  const { tool } = toolWith([{ exitCode: 0, stdout: JSON.stringify({ state: 'purple' }) }])
  await assert.rejects(
    () => tool.execute({ action: 'status' }, { signal: undefined }),
    /unrecognized state/,
  )
})
