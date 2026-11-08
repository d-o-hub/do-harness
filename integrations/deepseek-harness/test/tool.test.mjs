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

function verifyResponse(overrides = {}) {
  return {
    exitCode: 0,
    stdout: JSON.stringify({
      ok: true,
      signal_set: 'feedback',
      failed: [],
      sensors: [{ name: 'fmt', ok: true, execution: 'ran', duration_ms: 4 }],
      ...overrides,
    }),
  }
}

function statusResponse(state = 'green', overrides = {}) {
  return {
    exitCode: 0,
    stdout: JSON.stringify({ state, reason: 'current', ...overrides }),
  }
}


test('validateArgs accepts the documented shape and rejects junk', () => {
  assert.deepEqual(validateArgs({ action: 'run', set: ' feedback ', changed: true }), {
    action: 'run',
    set: 'feedback',
    changed: true,
    since: undefined,
    task: undefined,
    reuse: false,
  })
  assert.deepEqual(validateArgs({ action: 'status' }), {
    action: 'status',
    set: undefined,
    changed: false,
    since: undefined,
    task: undefined,
    reuse: false,
  })
  assert.deepEqual(validateArgs({ action: 'explain', since: 'main' }), {
    action: 'explain',
    set: undefined,
    changed: false,
    since: 'main',
    task: undefined,
    reuse: false,
  })
  assert.deepEqual(validateArgs({ action: 'context', task: 12 }), {
    action: 'context',
    set: undefined,
    changed: false,
    since: undefined,
    task: 12,
    reuse: false,
  })
  assert.deepEqual(validateArgs({ action: 'run', task: 7, reuse: true }), {
    action: 'run',
    set: undefined,
    changed: false,
    since: undefined,
    task: 7,
    reuse: true,
  })
  assert.throws(() => validateArgs(null), /must be an object/)
  assert.throws(() => validateArgs({ action: 'exec' }), /action must be/)
  assert.throws(() => validateArgs({ action: 'run', set: '' }), /set must be/)
  assert.throws(() => validateArgs({ action: 'run', changed: 'yes' }), /changed must be/)
  assert.throws(() => validateArgs({ action: 'run', since: '' }), /since must be/)
  assert.throws(() => validateArgs({ action: 'run', task: 1.5 }), /positive safe integer/)
  assert.throws(() => validateArgs({ action: 'run', task: 0 }), /positive safe integer/)
  assert.throws(() => validateArgs({ action: 'run', reuse: 'yes' }), /reuse must be/)
  assert.throws(() => validateArgs({ action: 'run', reuse: true }), /reuse requires a task scope/)
  assert.throws(() => validateArgs({ action: 'context' }), /context requires a positive integer task/)
  assert.throws(() => validateArgs({ action: 'context', task: 1, set: 'x' }), /does not accept field set/)
  assert.throws(() => validateArgs({ action: 'status', task: 1 }), /does not accept field task/)
  assert.throws(() => validateArgs({ action: 'explain', reuse: true }), /does not accept field reuse/)
  assert.throws(() => validateArgs({ action: 'status', since: 'main' }), /does not accept field since/)
  assert.throws(() => validateArgs({ action: 'run', foo: true }), /does not accept field foo/)
})

test('execute maps actions to fixed argv and defaults', async () => {
  const { ctx, tool } = toolWith([
    verifyResponse(),
    statusResponse(),
    statusResponse(),
    {
      exitCode: 0,
      stdout: JSON.stringify({ set: 'verification', changed_files: [], selected: [], skipped: [] }),
    },
  ])

  const run = await tool.execute({ action: 'run' }, { signal: undefined })
  assert.equal(run.action, 'run')
  assert.equal(run.state, 'green')
  assert.equal(run.vacuous, false)
  const status = await tool.execute({ action: 'status' }, { signal: undefined })
  assert.equal(status.action, 'status')
  assert.equal(status.state, 'green')
  await tool.execute({ action: 'explain', changed: true }, { signal: undefined })

  const argvs = ctx.spawnedArgv.map((argv) => argv.join(' '))
  assert.ok(argvs[0].includes('verify --set feedback --format json'))
  assert.ok(argvs[1].includes('status --set feedback --format json'))
  assert.ok(argvs[2].includes('status --set verification --format json'))
  assert.ok(argvs[3].includes('explain --set verification --format json --changed'))
})

test('a zero-sensor run is green but explicitly vacuous', async () => {
  const { tool } = toolWith([verifyResponse({ sensors: [] }), statusResponse()])
  const value = await tool.execute({ action: 'run' }, { signal: undefined })
  assert.equal(value.state, 'green')
  assert.equal(value.vacuous, true)
  assert.deepEqual(value.passed, [])
  assert.equal(value.next_action, 'explain')
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
          { name: 'fmt', ok: true, execution: 'ran', duration_ms: 1 },
          { name: 'clippy', ok: false, execution: 'ran', duration_ms: 2 },
        ],
      }),
    },
    statusResponse('red', { reason: 'failures', evidence: { failed: ['clippy'] } }),
  ])
  const value = await tool.execute({ action: 'run', set: 'feedback' }, { signal: undefined })
  assert.equal(value.state, 'red')
  assert.deepEqual(value.failed, ['clippy'])
  assert.deepEqual(value.passed, ['fmt'])
  assert.equal(value.duration_ms, 3)
  assert.equal(value.next_action, 'verify')
})

test('task-scoped reuse maps to --record --unchanged=skip and reports reused beats', async () => {
  const { ctx, tool } = toolWith([
    verifyResponse({ sensors: [{ name: 'fmt', ok: true, execution: 'reused', reused_beat_id: 42, duration_ms: 0 }] }),
    statusResponse(),
  ])
  const value = await tool.execute({ action: 'run', set: 'feedback', task: 7, reuse: true }, { signal: undefined })
  assert.equal(value.state, 'green')
  assert.deepEqual(value.sensors, [
    { name: 'fmt', ok: true, execution: 'reused', reused_beat_id: 42 },
  ])
  assert.equal(value.complete, true)
  assert.equal(value.next_action, null)
  const argv = ctx.spawnedArgv[0].join(' ')
  assert.ok(argv.includes('verify --set feedback --format json'))
  assert.ok(argv.includes('--record'))
  assert.ok(argv.includes('--unchanged=skip'))
  assert.ok(argv.includes('--task 7'))
})

test('the completion set runs strict verify followed by status and rejects changed/since/reuse', async () => {
  const { ctx, tool } = toolWith([verifyResponse({ signal_set: 'verification' }), statusResponse()])
  const value = await tool.execute({ action: 'run', set: 'verification' }, { signal: undefined })
  assert.equal(value.state, 'green')
  assert.equal(value.complete, true)
  assert.equal(value.evidence.state, 'green')
  const argv = ctx.spawnedArgv[0].join(' ')
  assert.ok(argv.includes('verify --set verification --format json'))
  assert.ok(argv.includes('--strict'))
  assert.ok(argv.includes('--record'))
  assert.ok(argv.includes('--unchanged=run'))
  assert.ok(ctx.spawnedArgv[1].join(' ').includes('status --set verification --format json'))
  await assert.rejects(
    () => tool.execute({ action: 'run', set: 'verification', changed: true }, { signal: undefined }),
    /completion set does not accept/,
  )
  await assert.rejects(
    () => tool.execute({ action: 'run', set: 'verification', since: 'main' }, { signal: undefined }),
    /completion set does not accept/,
  )
  await assert.rejects(
    () => tool.execute({ action: 'run', set: 'verification', task: 2, reuse: true }, { signal: undefined }),
    /completion set does not accept/,
  )
})

test('run/explain pass --since through to the CLI', async () => {
  const { ctx, tool } = toolWith([
    verifyResponse(),
    statusResponse(),
    {
      exitCode: 0,
      stdout: JSON.stringify({
        set: 'all',
        selection_mode: 'range',
        since: 'main',
        changed_files: [],
        selected: [],
        skipped: [],
      }),
    },
  ])
  await tool.execute({ action: 'run', set: 'feedback', since: 'main' }, { signal: undefined })
  assert.ok(ctx.spawnedArgv[0].join(' ').includes('--since main'))
  await tool.execute({ action: 'explain', set: 'all', since: 'main' }, { signal: undefined })
  assert.ok(ctx.spawnedArgv[2].join(' ').includes('--since main'))
})

test('context returns bounded resume context and preserves unavailable outcomes', async () => {
  const document = { schema_version: 1, task: { id: 5 }, next_action: { kind: 'verify' } }
  const { ctx, tool } = toolWith([
    { exitCode: 0, stdout: JSON.stringify(document) },
    { exitCode: 1, stdout: '', stderr: 'task 999 not found' },
  ])
  const ok = await tool.execute({ action: 'context', task: 5 }, { signal: undefined })
  assert.equal(ok.state, 'ok')
  assert.equal(ok.document.task.id, 5)
  assert.equal(ok.next_action, 'verify')
  const argv = ctx.spawnedArgv[0].join(' ')
  assert.equal(argv, 'do-harness task context 5 --format json --max-bytes 8192')
  const unavailable = await tool.execute({ action: 'context', task: 999 }, { signal: undefined })
  assert.equal(unavailable.state, 'unavailable')
  assert.equal(unavailable.document, null)
  assert.match(unavailable.reason, /not found/)
})

test('status accepts the running state as an ordinary in-progress outcome', async () => {
  const { tool } = toolWith([statusResponse('running', { reason: 'inflight', running: { pid: 1 } })])
  const value = await tool.execute({ action: 'status' }, { signal: undefined })
  assert.equal(value.state, 'running')
  assert.equal(value.next_action, 'status')
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
