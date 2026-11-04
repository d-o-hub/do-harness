// The required lifecycle scenarios, each driving the real host against a
// disposable repository.

import { rmSync, writeFileSync } from 'node:fs'

import { runSession } from './host.mjs'

const SUCCEEDING_SENSOR = 'exit 0'
const FAILING_SENSOR = 'exit 1'

export const SCENARIO_RUNNERS = [
  {
    id: 'fresh-pass',
    async prepare({ repo, harness }) {
      writeConfig(repo, SUCCEEDING_SENSOR)
      await recordEvidence(repo, harness)
    },
    script: [{ finish: 'done' }],
  },
  {
    id: 'green-then-edit',
    async prepare({ repo, harness }) {
      writeConfig(repo, SUCCEEDING_SENSOR)
      await recordEvidence(repo, harness)
    },
    script: [{ tool: { name: 'conformance_probe', arguments: { action: 'edit-source' } } }, { finish: 'done' }],
  },
  {
    id: 'repair-and-reverify',
    async prepare({ repo, harness }) {
      writeConfig(repo, SUCCEEDING_SENSOR)
      await recordEvidence(repo, harness)
    },
    // A policy edit exercises the policy fingerprint; `green-then-edit`
    // covers the source-file fingerprint.
    script: [
      { tool: { name: 'conformance_probe', arguments: { action: 'edit-policy' } } },
      { tool: { name: 'development_signals', arguments: { action: 'run', set: 'verification' } } },
      { finish: 'done' },
    ],
  },
  {
    id: 'failing-sensor',
    async prepare({ repo, harness }) {
      writeConfig(repo, FAILING_SENSOR)
      await recordEvidence(repo, harness)
    },
    script: [{ finish: 'done' }],
  },
  {
    id: 'missing-or-malformed-evidence',
    async prepare({ repo, harness }) {
      writeConfig(repo, SUCCEEDING_SENSOR)
      await recordEvidence(repo, harness)
      rmSync(`${repo}/.do-harness/evidence.verification.json`, { force: true })
    },
    script: [{ finish: 'done' }],
  },
  {
    id: 'infrastructure-error',
    async prepare({ repo, harness }) {
      writeConfig(repo, SUCCEEDING_SENSOR)
      await recordEvidence(repo, harness)
    },
    completionSignalSet: 'not-a-signal-set',
    script: [{ finish: 'done' }],
  },
  {
    id: 'retry-exhaustion',
    maxContinuations: 1,
    async prepare({ repo, harness }) {
      writeConfig(repo, SUCCEEDING_SENSOR)
      await recordEvidence(repo, harness)
      rmSync(`${repo}/.do-harness/evidence.verification.json`, { force: true })
    },
    script: [{ finish: 'done' }],
  },
  {
    id: 'plugin-load-failure',
    unmounted: true,
    binary: '/nonexistent/do-harness',
    async prepare({ repo }) {
      writeConfig(repo, SUCCEEDING_SENSOR)
    },
    script: [{ finish: 'done' }],
  },
  {
    id: 'user-cancellation',
    async prepare({ repo, harness }) {
      writeConfig(repo, SUCCEEDING_SENSOR)
      await recordEvidence(repo, harness)
    },
    script: [{ cancel: 'user' }],
  },
]

/** Writes a one-sensor configuration so evidence is cheap and deterministic. */
export function writeConfig(repo, command) {
  writeFileSync(
    `${repo}/do-harness.toml`,
    [
      'language = "generic"',
      '',
      '[signal-sets]',
      'verification = ["probe"]',
      '',
      '[[sensors]]',
      'name = "probe"',
      `argv = ["bash", "-c", "${command}"]`,
      '',
    ].join('\n'),
  )
}

/** Runs the real binary to create the evidence the session will be judged on. */
export async function recordEvidence(repo, harness) {
  await harness(['verify', '--set', 'verification', '--record', '--format', 'json'], repo, {
    tolerateFailure: true,
  })
}

/** Runs one scenario end to end, returning the raw host observation. */
export async function runScenario({ scenario, host, repo, harness, patch, out, script }) {
  await scenario.prepare({ repo, harness })
  const session = await runSession({
    dsh: host.dsh,
    home: host.home,
    cwd: repo,
    patch,
    script: script ?? scenario.script,
    out,
  })
  return summarize({ scenario, session, harness, repo })
}

/** Reduces the raw host transcript to the fields the contract records. */
async function summarize({ scenario, session, harness, repo }) {
  const calls = session.events.filter((event) => event.event === 'model-call')
  const continuations = calls.filter((call) => call.harness_messages.length > 0).length
  const errors = session.events.filter((event) => event.event === 'agent-error')
  const cancelRequested = session.events.some((event) => event.event === 'cancel-requested')
  const infrastructure = calls.some((call) =>
    call.harness_messages.some((message) => message.includes('could not evaluate')),
  )
  const created = session.events.find((event) => event.event === 'agent-created')
  const mounted = Array.isArray(created?.tools) && created.tools.includes('development_signals')
  const set = scenario.completionSignalSet ?? 'verification'
  const status = await harness(['status', '--set', set, '--format', 'json'], repo, { tolerateFailure: true })
  const evidenceGreenAtClose = status.state === 'green'
  return {
    id: scenario.id,
    observed: {
      terminal: terminalFor({ scenario, session, errors, mounted, cancelRequested }),
      continuations,
      evidenceGreenAtClose,
      error: session.timedOut ? 'timeout' : null,
      infrastructure,
      reason: infrastructure ? 'infrastructure-error' : null,
    },
    detail: {
      exitCode: session.exitCode,
      signal: session.signal,
      mounted,
      errors: errors.map((event) => event.message.slice(0, 200)),
      stderr: session.stderr.split('\n').find((line) => line.trim() !== '') ?? '',
    },
  }
}

function terminalFor({ scenario, session, errors, mounted, cancelRequested }) {
  if (scenario.unmounted) {
    return mounted ? 'verified' : 'unsupported'
  }
  if (session.timedOut) {
    return 'unsupported'
  }
  if (cancelRequested && session.exitCode !== 0 && errors.length === 0) {
    return 'cancelled'
  }
  if (errors.length > 0 && session.exitCode !== 0) {
    return 'blocked'
  }
  if (session.exitCode === 0 && errors.length === 0) {
    return 'verified'
  }
  return 'blocked'
}
