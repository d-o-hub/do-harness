// End-to-end dogfood: the DSH bundle drives the real do-harness binary
// through the central evidence contract — green -> edit -> stale -> verify ->
// green — plus the strict completion gate.

import assert from 'node:assert/strict'
import { spawnSync } from 'node:child_process'
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import test from 'node:test'

import { apply } from '../index.js'
import { createFakeAgent, createFakeCtx } from './fake-ctx.mjs'

const BIN = process.env.DO_HARNESS_BIN ?? 'do-harness'

/**
 * Skip reason for the e2e suite, or `false` to run.
 *
 * An explicit `DO_HARNESS_BIN` is a hard contract: tests must fail, not skip,
 * when the named binary cannot run. Only auto-discovery may skip.
 */
function e2eSkip() {
  const probe = spawnSync(BIN, ['version', '--format', 'json'], { encoding: 'utf8' })
  if (probe.status === 0 || process.env.DO_HARNESS_BIN) {
    return false
  }
  return 'do-harness not on PATH (set DO_HARNESS_BIN to run the e2e suite)'
}

/** Runs one git command in `cwd`, asserting success. */
function git(cwd, args) {
  const env = {
    ...process.env,
    GIT_CONFIG_NOSYSTEM: '1',
    GIT_AUTHOR_NAME: 't',
    GIT_AUTHOR_EMAIL: 't@t',
    GIT_COMMITTER_NAME: 't',
    GIT_COMMITTER_EMAIL: 't@t',
  }
  // Tests may run under a git hook; the fixture repo must not inherit the
  // hook's repository environment.
  for (const key of [
    'GIT_DIR',
    'GIT_WORK_TREE',
    'GIT_INDEX_FILE',
    'GIT_OBJECT_DIRECTORY',
    'GIT_ALTERNATE_OBJECT_DIRECTORIES',
    'GIT_CEILING_DIRECTORIES',
    'GIT_NAMESPACE',
    'GIT_PREFIX',
  ]) {
    delete env[key]
  }
  const result = spawnSync('git', args, { cwd, encoding: 'utf8', env })
  assert.equal(result.status, 0, `git ${args.join(' ')}: ${result.stderr}`)
}

/** Creates a committed fixture repo with feedback + verification signals. */
function fixtureRepo() {
  const dir = mkdtempSync(join(tmpdir(), 'dsh-do-harness-e2e-'))
  writeFileSync(
    join(dir, 'do-harness.toml'),
    [
      '[signal-sets]',
      'verification = ["marker"]',
      'feedback = ["marker"]',
      '',
      '[[sensors]]',
      'name = "marker"',
      'argv = ["true"]',
      'inputs = ["lib.rs"]',
      '',
    ].join('\n'),
  )
  writeFileSync(join(dir, 'lib.rs'), 'v1\n')
  git(dir, ['init', '-q'])
  git(dir, ['add', '-A'])
  git(dir, ['commit', '-qm', 'test: base'])
  return dir
}

/** Runs the real harness binary; returns { status, stdout, stderr }. */
function harness(dir, args, env = {}) {
  const childEnv = { ...process.env, ...env }
  for (const key of ['GIT_DIR', 'GIT_WORK_TREE', 'GIT_INDEX_FILE', 'GIT_OBJECT_DIRECTORY', 'GIT_ALTERNATE_OBJECT_DIRECTORIES', 'GIT_CEILING_DIRECTORIES', 'GIT_NAMESPACE', 'GIT_PREFIX']) {
    delete childEnv[key]
  }
  const result = spawnSync(BIN, args, { cwd: dir, encoding: 'utf8', env: childEnv })
  assert.equal(result.status, 0, `do-harness ${args.join(' ')}: ${result.stderr}`)
  return result.stdout
}

/** Creates a task and returns its numeric id. */
function addTask(dir) {
  const stdout = harness(dir, ['task', 'add', 'e2e task'])
  const match = /Added task (\d+)/.exec(stdout)
  assert.ok(match, `could not parse task id from: ${stdout}`)
  return Number(match[1])
}

/** Mounts the bundle against `dir` with real subprocess execution. */
async function mount(dir, { strictCompletion = true } = {}) {
  const ctx = createFakeCtx({ realSpawn: true })
  const config = { workspace: dir, strictCompletion }
  if (process.env.DO_HARNESS_BIN) {
    config.binary = process.env.DO_HARNESS_BIN
  }
  await apply(ctx, config)
  return ctx
}

test(
  'green -> edit -> stale -> verify -> green through the bundle',
  { skip: e2eSkip() },
  async (t) => {
    const dir = fixtureRepo()
    t.after(() => rmSync(dir, { recursive: true, force: true }))
    const ctx = await mount(dir)
    const tool = ctx.tool('development_signals')
    assert.ok(tool, 'development_signals must be registered')

    // Missing: no evidence yet.
    const missing = await tool.execute({ action: 'status', set: 'verification' }, {})
    assert.equal(missing.state, 'missing')

    // Gate refuses completion while evidence is missing.
    const agent = createFakeAgent('e2e-agent')
    await ctx.emit('agent/turn-stopping', { agent, turn: 1, signal: undefined })
    assert.equal(agent.steered.length, 1)
    assert.match(agent.steered[0].content[0].text, /missing|No current verification evidence/)
    agent.steered.length = 0

    // Run verification: evidence is generated and status turns green.
    const run = await tool.execute({ action: 'run', set: 'verification' }, {})
    assert.equal(run.state, 'green', JSON.stringify(run))
    assert.deepEqual(run.failed, [])
    const green = await tool.execute({ action: 'status', set: 'verification' }, {})
    assert.equal(green.state, 'green')

    await ctx.emit('agent/turn-stopping', { agent, turn: 2, signal: undefined })
    assert.equal(agent.steered.length, 0, 'green evidence permits stop')

    // An uncommitted edit makes the evidence stale.
    writeFileSync(join(dir, 'lib.rs'), 'v2 changed\n')
    const stale = await tool.execute({ action: 'status', set: 'verification' }, {})
    assert.equal(stale.state, 'stale')
    assert.equal(stale.reason, 'workspace_changed')

    await ctx.emit('agent/turn-stopping', { agent, turn: 3, signal: undefined })
    assert.equal(agent.steered.length, 1)
    assert.match(agent.steered[0].content[0].text, /stale|workspace changed/)
    agent.steered.length = 0

    // Re-verify: green again.
    const rerun = await tool.execute({ action: 'run', set: 'verification' }, {})
    assert.equal(rerun.state, 'green')
    await ctx.emit('agent/turn-stopping', { agent, turn: 4, signal: undefined })
    assert.equal(agent.steered.length, 0)

    // Every invocation went through the subprocess seam with explicit argv.
    assert.ok(ctx.spawnedArgv.every((argv) => Array.isArray(argv) && argv.length > 1))
  },
)

test(
  'a failing signal returns red through the tool and blocks completion',
  { skip: e2eSkip() },
  async (t) => {
    const dir = fixtureRepo()
    t.after(() => rmSync(dir, { recursive: true, force: true }))
    // Swap the sensor to one that fails, then re-run.
    writeFileSync(
      join(dir, 'do-harness.toml'),
      [
        '[signal-sets]',
        'verification = ["bad"]',
        '',
        '[[sensors]]',
        'name = "bad"',
        'argv = ["false"]',
        '',
      ].join('\n'),
    )
    git(dir, ['add', '-A'])
    git(dir, ['commit', '-qm', 'test: failing sensor'])
    const ctx = await mount(dir)
    const tool = ctx.tool('development_signals')

    const run = await tool.execute({ action: 'run', set: 'verification' }, {})
    assert.equal(run.state, 'red')
    assert.deepEqual(run.failed, ['bad'])

    const status = await tool.execute({ action: 'status', set: 'verification' }, {})
    assert.equal(status.state, 'red')

    const agent = createFakeAgent('e2e-red')
    await ctx.emit('agent/turn-stopping', { agent, turn: 1, signal: undefined })
    assert.equal(agent.steered.length, 1)
    assert.match(agent.steered[0].content[0].text, /bad/)
  },
)

test(
  'a docs-only change selects only the docs signal',
  { skip: e2eSkip() },
  async (t) => {
    const dir = mkdtempSync(join(tmpdir(), 'dsh-do-harness-explain-'))
    t.after(() => rmSync(dir, { recursive: true, force: true }))
    writeFileSync(
      join(dir, 'do-harness.toml'),
      [
        '[signal-sets]',
        'all = ["rs-check", "docs"]',
        '',
        '[[sensors]]',
        'name = "rs-check"',
        'argv = ["true"]',
        'when-changed = ["**/*.rs"]',
        '',
        '[[sensors]]',
        'name = "docs"',
        'argv = ["true"]',
        'when-changed = ["**/*.md"]',
        '',
      ].join('\n'),
    )
    writeFileSync(join(dir, 'main.rs'), 'fn main() {}\n')
    writeFileSync(join(dir, 'README.md'), '# docs\n')
    git(dir, ['init', '-q'])
    git(dir, ['add', '-A'])
    git(dir, ['commit', '-qm', 'test: base'])
    writeFileSync(join(dir, 'README.md'), '# docs v2\n')

    const ctx = await mount(dir)
    const tool = ctx.tool('development_signals')
    const explain = await tool.execute({ action: 'explain', set: 'all', changed: true }, {})
    assert.deepEqual(
      explain.selected.map((entry) => entry.name),
      ['docs'],
    )
    assert.deepEqual(
      explain.skipped.map((entry) => entry.name),
      ['rs-check'],
    )
  },
)
test(
  'a task-scoped run reuses an eligible beat; source, policy, and declared environment edits invalidate it',
  { skip: e2eSkip() },
  async (t) => {
    const dir = fixtureRepo()
    t.after(() => rmSync(dir, { recursive: true, force: true }))
    const task = addTask(dir)
    const ctx = await mount(dir)
    const tool = ctx.tool('development_signals')

    // First run executes and records a task-scoped beat.
    const first = await tool.execute({ action: 'run', set: 'feedback', task, reuse: true }, {})
    assert.equal(first.state, 'green')
    assert.deepEqual(first.sensors, [{ name: 'marker', ok: true, execution: 'ran' }])

    // Eligible reuse: unchanged scope and inputs skip execution visibly.
    const second = await tool.execute({ action: 'run', set: 'feedback', task, reuse: true }, {})
    assert.equal(second.state, 'green')
    assert.equal(second.sensors[0].execution, 'reused')
    assert.ok(Number.isInteger(second.sensors[0].reused_beat_id), 'reused beat id must be reported')
    assert.equal(second.evidence.state, 'red')

    // Source change invalidates reuse: the sensor must execute again.
    writeFileSync(join(dir, 'lib.rs'), 'v2 changed\n')
    const afterSource = await tool.execute({ action: 'run', set: 'feedback', task, reuse: true }, {})
    assert.equal(afterSource.sensors[0].execution, 'ran')

    // Policy change invalidates reuse.
    writeFileSync(join(dir, 'do-harness.toml'), '\n# policy edit\n', { flag: 'a' })
    const afterPolicy = await tool.execute({ action: 'run', set: 'feedback', task, reuse: true }, {})
    assert.equal(afterPolicy.sensors[0].execution, 'ran')
  },
)

test(
  'a declared environment change invalidates reuse',
  { skip: e2eSkip() },
  async (t) => {
    const dir = mkdtempSync(join(tmpdir(), 'dsh-do-harness-env-'))
    t.after(() => rmSync(dir, { recursive: true, force: true }))
    writeFileSync(
      join(dir, 'do-harness.toml'),
      [
        '[signal-sets]',
        'feedback = ["envsensor"]',
        '',
        '[[sensors]]',
        'name = "envsensor"',
        'argv = ["true"]',
        'inputs = ["lib.rs"]',
        'environment-inputs = ["MY_FLAG"]',
        '',
      ].join('\n'),
    )
    git(dir, ['init', '-q'])
    git(dir, ['add', '-A'])
    git(dir, ['commit', '-qm', 'test: base'])
    const task = addTask(dir)
    const ctx = await mount(dir)
    const tool = ctx.tool('development_signals')

    process.env.MY_FLAG = 'one'
    try {
      const first = await tool.execute({ action: 'run', set: 'feedback', task, reuse: true }, {})
      assert.equal(first.state, 'green')
      // A different declared environment value can never reuse the earlier beat:
      // it must execute, even when reuse was requested.
      process.env.MY_FLAG = 'two'
      const changed = await tool.execute({ action: 'run', set: 'feedback', task, reuse: true }, {})
      assert.equal(changed.sensors[0].execution, 'ran')
    } finally {
      delete process.env.MY_FLAG
    }
  },
)

test(
  'a newer failing observation is not hidden by an older passing beat',
  { skip: e2eSkip() },
  async (t) => {
    const dir = mkdtempSync(join(tmpdir(), 'dsh-do-harness-shadow-'))
    t.after(() => rmSync(dir, { recursive: true, force: true }))
    writeFileSync(join(dir, 'src.txt'), 'ok\n')
    writeFileSync(
      join(dir, 'do-harness.toml'),
      [
        '[signal-sets]',
        'feedback = ["content"]',
        '',
        '[[sensors]]',
        'name = "content"',
        'argv = ["bash", "-c", "grep -q ok src.txt"]',
        'inputs = ["src.txt"]',
        '',
      ].join('\n'),
    )
    git(dir, ['init', '-q'])
    git(dir, ['add', '-A'])
    git(dir, ['commit', '-qm', 'test: base'])
    const task = addTask(dir)
    const ctx = await mount(dir)
    const tool = ctx.tool('development_signals')

    const passing = await tool.execute({ action: 'run', set: 'feedback', task, reuse: true }, {})
    assert.equal(passing.state, 'green')
    writeFileSync(join(dir, 'src.txt'), 'bad\n')
    const failing = await tool.execute({ action: 'run', set: 'feedback', task, reuse: true }, {})
    assert.equal(failing.state, 'red')
    assert.deepEqual(failing.failed, ['content'])
    const status = await tool.execute({ action: 'status', set: 'feedback' }, {})
    assert.equal(status.state, 'red', 'the older beat must not hide the newer failure')
  },
)

test(
  'a missing dependency forces execution and never authorizes stale reuse',
  { skip: e2eSkip() },
  async (t) => {
    const dir = mkdtempSync(join(tmpdir(), 'dsh-do-harness-missingdep-'))
    t.after(() => rmSync(dir, { recursive: true, force: true }))
    writeFileSync(
      join(dir, 'do-harness.toml'),
      [
        '[signal-sets]',
        'feedback = ["probe", "missing-tool"]',
        '',
        '[[sensors]]',
        'name = "probe"',
        'argv = ["true"]',
        '',
        '[[sensors]]',
        'name = "missing-tool"',
        'argv = ["/nonexistent/tool"]',
        '',
      ].join('\n'),
    )
    git(dir, ['init', '-q'])
    git(dir, ['add', '-A'])
    git(dir, ['commit', '-qm', 'test: base'])
    const task = addTask(dir)
    const ctx = await mount(dir)
    const tool = ctx.tool('development_signals')

    const first = await tool.execute({ action: 'run', set: 'feedback', task, reuse: true }, {})
    assert.equal(first.state, 'red')
    const missing = first.sensors.find((entry) => entry.name === 'missing-tool')
    assert.equal(missing.execution, 'ran')
    assert.equal(missing.ok, false)

    // Reuse must not invent a pass for the sensor that never succeeded.
    const second = await tool.execute({ action: 'run', set: 'feedback', task, reuse: true }, {})
    const again = second.sensors.find((entry) => entry.name === 'missing-tool')
    assert.notEqual(again.execution, 'reused')
    assert.equal(again.ok, false)
  },
)

test(
  'context returns bounded resume context; unavailable tasks are preserved, not success',
  { skip: e2eSkip() },
  async (t) => {
    const dir = fixtureRepo()
    t.after(() => rmSync(dir, { recursive: true, force: true }))
    const task = addTask(dir)
    const ctx = await mount(dir)
    const tool = ctx.tool('development_signals')

    const value = await tool.execute({ action: 'context', task }, {})
    assert.equal(value.state, 'ok')
    assert.equal(value.document.task.id, task)
    assert.ok(value.document.budget.max_bytes === 8192, 'the context budget must be the fixed bound')
    const rendered = JSON.stringify(value)
    assert.ok(Buffer.byteLength(rendered) <= 9600, `context must be bounded: ${Buffer.byteLength(rendered)}`)
    assert.ok(['verify', 'status', 'explain', 'task context', null].includes(value.next_action))

    const unavailable = await tool.execute({ action: 'context', task: 987654321 }, {})
    assert.equal(unavailable.state, 'unavailable')
    assert.equal(unavailable.document, null)
  },
)

test(
  'strict completion requires fresh green non-vacuous evidence and is followed by status',
  { skip: e2eSkip() },
  async (t) => {
    const dir = fixtureRepo()
    t.after(() => rmSync(dir, { recursive: true, force: true }))
    const ctx = await mount(dir)
    const tool = ctx.tool('development_signals')

    const passing = await tool.execute({ action: 'run', set: 'verification' }, {})
    assert.equal(passing.state, 'green')
    assert.equal(passing.complete, true)
    assert.equal(passing.evidence.state, 'green')
    assert.equal(passing.next_action, null)
    const verifyArgv = ctx.spawnedArgv.find((argv) => argv.join(' ').includes('verify'))
    assert.ok(verifyArgv.join(' ').includes('--unchanged=run'))

    writeFileSync(
      join(dir, 'do-harness.toml'),
      [
        '[signal-sets]',
        'verification = ["bad"]',
        '',
        '[[sensors]]',
        'name = "bad"',
        'argv = ["false"]',
        '',
      ].join('\n'),
    )
    const failing = await tool.execute({ action: 'run', set: 'verification' }, {})
    assert.equal(failing.state, 'red')
    assert.equal(failing.complete, false)
    assert.equal(failing.next_action, 'verify')
    const status = await tool.execute({ action: 'status', set: 'verification' }, {})
    assert.equal(status.state, 'red')
  },
)

test(
  'negative control: with the stop integration removed, stale evidence does not withhold completion',
  { skip: e2eSkip() },
  async (t) => {
    const dir = fixtureRepo()
    t.after(() => rmSync(dir, { recursive: true, force: true }))
    const ctx = await mount(dir, { strictCompletion: false })
    const tool = ctx.tool('development_signals')
    assert.equal(ctx.listeners.has('agent/turn-stopping'), false)
    await tool.execute({ action: 'run', set: 'verification' }, {})
    writeFileSync(join(dir, 'lib.rs'), 'v2 changed\n')
    const agent = createFakeAgent('e2e-negative')
    await ctx.emit('agent/turn-stopping', { agent, turn: 1, signal: undefined })
    assert.equal(agent.steered.length, 0, 'without the gate the loop closes even with stale evidence')
  },
)
