#!/usr/bin/env node
// Real-host completion-gating conformance runner.
//
// Usage:
//   node conformance/run.mjs --host /path/to/dsh-install --binary /path/to/do-harness
//     [--json result.json] [--required] [--keep]
//
// Ordinary CI never runs this: it needs a pinned client install. It launches no
// paid model and contacts no network service; every turn is synthetic.

import { spawnSync } from 'node:child_process'
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { dirname, join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

import { buildResult, validateResult } from '../lib/conformance.mjs'
import { adapterDigest, readAdapterIdentity, readClientVersion, CLIENT_ID, ensureProfile, writePatch } from './host.mjs'
import { recordEvidence, runScenario, SCENARIO_RUNNERS, writeConfig } from './scenarios.mjs'

const HERE = dirname(fileURLToPath(import.meta.url))
const BUNDLE_DIR = resolve(HERE, '..')
const PROVIDER_PLUGIN = join(HERE, 'scripted-provider.mjs')

/** The client version these observations are pinned to. */
export const PINNED_CLIENT_VERSION = '0.1.7-rc.2'

async function main(argv) {
  const options = parseArgs(argv)
  const hostDir = resolve(options.host ?? process.env.DSH_HOST_DIR ?? process.cwd())
  const dsh = join(hostDir, 'node_modules', '.bin', 'dsh')
  const binary = resolve(options.binary ?? process.env.DO_HARNESS_BIN ?? join(BUNDLE_DIR, '..', '..', 'target', 'debug', 'do-harness'))

  const version = readClientVersion(hostDir)
  if (version !== PINNED_CLIENT_VERSION && !options.allowVersion) {
    throw new Error(
      `client ${CLIENT_ID} ${version} is not the pinned ${PINNED_CLIENT_VERSION}; ` +
        'rerun against the pinned install or pass --allow-version to record an unpinned observation',
    )
  }

  const runDir = mkdtempSync(join(tmpdir(), 'dsh-conformance-'))
  if (options.keep) {
    process.stderr.write(`keeping run directory: ${runDir}\n`)
  }
  const repo = join(runDir, 'repo')
  const home = join(runDir, 'home')
  mkdirSync(repo)
  mkdirSync(home)
  initRepo(repo)

  const adapter = readAdapterIdentity(BUNDLE_DIR)
  const digest = adapterDigest(BUNDLE_DIR)
  const harness = makeHarness(binary)
  const harnessIdentity = await harness(['version', '--format', 'json'], repo)
  writeConfig(repo, 'exit 0')
  await recordEvidence(repo, harness)
  const evidenceSchema = readEvidenceSchema(repo)

  await ensureProfile({ dsh, home, cwd: repo })

  if (options.negative) {
    return runNegative({ scenarioId: 'green-then-edit', runDir, repo, dsh, home, binary, harness })
  }

  const observations = []
  for (const scenario of SCENARIO_RUNNERS) {
    const patch = join(runDir, `${scenario.id}.yml`)
    writePatch({
      path: patch,
      providerPlugin: PROVIDER_PLUGIN,
      bundle: join(BUNDLE_DIR, 'index.js'),
      binary: scenario.binary ?? binary,
      workspace: repo,
      maxContinuations: scenario.maxContinuations ?? 1,
      completionSignalSet: scenario.completionSignalSet ?? 'verification',
    })
    const out = join(runDir, `${scenario.id}.jsonl`)
    const result = await runScenario({ scenario, host: { dsh, home }, repo, harness, binary, patch, out })
    observations.push({ id: result.id, ...result.observed })
    process.stderr.write(
      `scenario ${scenario.id}: ${result.observed.terminal} ` +
        `(exit=${result.detail.exitCode}, mounted=${result.detail.mounted}) ${result.detail.stderr}\n`,
    )
    if (!options.keep) {
      resetRepo(repo)
    }
  }

  const document = validateResult(
    buildResult({
      client: { id: CLIENT_ID, version },
      adapter: { ...adapter, contentDigest: digest },
      harness: { version: harnessIdentity.version, evidenceSchema },
      observations,
    }),
  )
  const target = options.json ? resolve(options.json) : null
  if (target) {
    writeFileSync(target, `${JSON.stringify(document, null, 2)}\n`)
  }
  process.stdout.write(`${JSON.stringify(document, null, 2)}\n`)
  if (!options.keep) {
    rmSync(runDir, { recursive: true, force: true })
  }
  return exitCodeFor(document, options)
}

/**
 * Negative control: the same lifecycle with the stop integration removed must
 * be reported as a failure. A conformance suite that passes without the
 * adapter proves nothing.
 */
async function runNegative({ scenarioId, runDir, repo, dsh, home, binary, harness }) {
  const scenario = SCENARIO_RUNNERS.find((entry) => entry.id === scenarioId)
  const patch = join(runDir, 'negative.yml')
  writePatch({
    path: patch,
    providerPlugin: PROVIDER_PLUGIN,
    bundle: join(BUNDLE_DIR, 'index.js'),
    binary,
    workspace: repo,
    maxContinuations: 1,
    strictCompletion: false,
  })
  const out = join(runDir, 'negative.jsonl')
  const result = await runScenario({ scenario, host: { dsh, home }, repo, harness, binary, patch, out })
  const detected = result.observed.terminal === 'verified' && result.observed.continuations === 0
  process.stderr.write(
    `negative control (${scenarioId} without the stop integration): ` +
      `terminal=${result.observed.terminal} continuations=${result.observed.continuations} ` +
      `${detected ? 'detected' : 'NOT detected'}\n`,
  )
  return detected ? 0 : 1
}

function exitCodeFor(document, options) {
  if (document.verdict === 'pass') {
    return 0
  }
  if (options.required) {
    return document.verdict === 'unknown' ? 2 : 1
  }
  return 0
}

function makeHarness(binary) {
  return async (args, cwd, { tolerateFailure = false } = {}) => {
    const outcome = spawnSync(binary, args, { cwd, encoding: 'utf8' })
    if (outcome.status !== 0 && !tolerateFailure) {
      throw new Error(`do-harness ${args.join(' ')} exited ${outcome.status}: ${outcome.stderr?.slice(-300)}`)
    }
    try {
      return JSON.parse(outcome.stdout)
    } catch {
      if (tolerateFailure) {
        return {}
      }
      throw new Error(`do-harness ${args.join(' ')} did not return JSON`)
    }
  }
}

function readEvidenceSchema(repo) {
  const path = join(repo, '.do-harness', 'evidence.verification.json')
  const version = JSON.parse(readFileSync(path, 'utf8')).schema_version
  if (!Number.isInteger(version)) {
    throw new Error(`could not read the evidence schema version from ${path}`)
  }
  return version
}

function initRepo(repo) {
  for (const argv of [
    ['init', '-q'],
    ['config', 'user.email', 'conformance@example.invalid'],
    ['config', 'user.name', 'conformance'],
  ]) {
    spawnSync('git', argv, { cwd: repo })
  }
  writeFileSync(join(repo, 'src.txt'), 'seed\n')
  spawnSync('git', ['add', '-A'], { cwd: repo })
  spawnSync('git', ['commit', '-qm', 'seed'], { cwd: repo })
}

function resetRepo(repo) {
  rmSync(join(repo, '.do-harness'), { recursive: true, force: true })
  spawnSync('git', ['checkout', '--', '.'], { cwd: repo })
  spawnSync('git', ['clean', '-qfd'], { cwd: repo })
}

function parseArgs(argv) {
  const options = {}
  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index]
    switch (arg) {
      case '--host':
        options.host = argv[++index]
        break
      case '--binary':
        options.binary = argv[++index]
        break
      case '--json':
        options.json = argv[++index]
        break
      case '--required':
        options.required = true
        break
      case '--keep':
        options.keep = true
        break
      case '--allow-version':
        options.allowVersion = true
        break
      case '--negative':
        options.negative = true
        break
      case '--pinned-version':
        process.stdout.write(`${PINNED_CLIENT_VERSION}\n`)
        process.exit(0)
        break
      default:
        throw new Error(`unknown argument: ${arg}`)
    }
  }
  return options
}

main(process.argv.slice(2)).then(
  (code) => process.exit(code),
  (error) => {
    process.stderr.write(`completion conformance failed: ${error.message}\n`)
    process.exit(2)
  },
)
