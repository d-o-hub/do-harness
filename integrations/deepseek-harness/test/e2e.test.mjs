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

/** Creates a committed fixture repo with an instant verification sensor. */
function fixtureRepo() {
  const dir = mkdtempSync(join(tmpdir(), 'dsh-do-harness-e2e-'))
  writeFileSync(
    join(dir, 'do-harness.toml'),
    [
      '[signal-sets]',
      'verification = ["marker"]',
      '',
      '[[sensors]]',
      'name = "marker"',
      'argv = ["true"]',
      '',
    ].join('\n'),
  )
  writeFileSync(join(dir, 'lib.rs'), 'v1\n')
  git(dir, ['init', '-q'])
  git(dir, ['add', '-A'])
  git(dir, ['commit', '-qm', 'test: base'])
  return dir
}

/** Mounts the bundle against `dir` with real subprocess execution. */
async function mount(dir) {
  const ctx = createFakeCtx({ realSpawn: true })
  const config = { workspace: dir, strictCompletion: true }
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
