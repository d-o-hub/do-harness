// Binary discovery, managed subprocess execution, and JSON validation for the
// do-harness CLI.
//
// All execution goes through the DSH `ctx.subprocess` seam with an explicit
// argv array; no shell is ever involved. Verification failures (exit 1) are
// domain outcomes returned to the model, while spawn/parse/config failures
// throw actionable errors that become tool errors or gate diagnostics.

export const PLUGIN_NAME = 'do-harness'

const MAX_STDOUT_BYTES = 4 * 1024 * 1024
const MAX_STDERR_BYTES = 256 * 1024
const RUN_GRACE_MS = 15 * 60 * 1000
const QUICK_GRACE_MS = 30 * 1000
const PROBE_GRACE_MS = 10 * 1000
const TAIL_LINES = 20

/** Resolves the do-harness executable: config > env > PATH name. */
export function resolveBinary(config, env = process.env) {
  if (config && typeof config.binary === 'string' && config.binary.trim() !== '') {
    return config.binary.trim()
  }
  if (env && typeof env.DO_HARNESS_BIN === 'string' && env.DO_HARNESS_BIN !== '') {
    return env.DO_HARNESS_BIN
  }
  return PLUGIN_NAME
}

/** Runs one do-harness argv through `ctx.subprocess` and collects output. */
export async function runHarness({ ctx, binary, cwd, args, signal, graceMs }) {
  let handle
  try {
    handle = ctx.subprocess.spawn({
      argv: [binary, ...args],
      cwd,
      stdio: {
        stdin: 'ignore',
        stdout: { maxBytes: MAX_STDOUT_BYTES },
        stderr: { maxBytes: MAX_STDERR_BYTES },
      },
      graceMs: graceMs ?? RUN_GRACE_MS,
      signal,
    })
  } catch (error) {
    throw new Error(
      `do-harness: failed to start \`${binary}\` (${args.join(' ')}): ${error.message}`,
    )
  }
  const outcome = await handle.done
  const stdout = readCollected(handle, 'stdout')
  const stderr = readCollected(handle, 'stderr')
  return { exitCode: outcome.exitCode, signal: outcome.signal, stdout, stderr }
}

/** Probes the binary at plugin load so a missing tool fails loudly. */
export async function probeBinary({ ctx, binary, cwd }) {
  let result
  try {
    result = await runHarness({
      ctx,
      binary,
      cwd,
      args: ['version', '--format', 'json'],
      graceMs: PROBE_GRACE_MS,
    })
  } catch (error) {
    throw new Error(
      `do-harness: cannot execute \`${binary}\` from ${cwd}. Install do-harness, ` +
        `set DO_HARNESS_BIN, or set the bundle's \`binary\` config. (${error.message})`,
    )
  }
  if (result.exitCode !== 0) {
    throw new Error(
      `do-harness: \`${binary} version\` exited ${result.exitCode}; the binary is not runnable. ` +
        `stderr: ${tail(result.stderr)}`,
    )
  }
  return binary
}

/** Reads a collected stream's full retained text (empty when not collected). */
function readCollected(handle, stream) {
  const reader = handle.collected ? handle.collected[stream] : undefined
  if (!reader || typeof reader.readFrom !== 'function') {
    return ''
  }
  const read = reader.readFrom(0)
  return typeof read.text === 'string' ? read.text : ''
}

/** Parses one JSON document from do-harness stdout, failing with context. */
export function parseHarnessJson(stdout, what) {
  try {
    return JSON.parse(stdout)
  } catch (error) {
    throw new Error(
      `do-harness ${what} did not return JSON: ${error.message}\n${tail(stdout)}`,
    )
  }
}

/** Validates a `status` document; throws on shapes the tool cannot trust. */
export function assertStatusDocument(document) {
  const states = new Set(['green', 'red', 'stale', 'missing'])
  if (
    !document ||
    typeof document !== 'object' ||
    Array.isArray(document) ||
    !states.has(document.state)
  ) {
    throw new Error(`do-harness status returned an unrecognized state: ${tail(JSON.stringify(document))}`)
  }
  if (typeof document.reason !== 'string') {
    throw new Error('do-harness status document is missing its reason')
  }
  if (document.evidence !== undefined && document.evidence !== null) {
    const failed = document.evidence.failed
    if (failed !== undefined && !Array.isArray(failed)) {
      throw new Error('do-harness status evidence.failed must be an array when present')
    }
  }
  return document
}

/** Validates a `verify` report and projects it to the tool's canonical value. */
export function toRunResult(document, requestedSet) {
  if (
    !document ||
    typeof document !== 'object' ||
    Array.isArray(document) ||
    typeof document.ok !== 'boolean' ||
    !Array.isArray(document.sensors) ||
    !Array.isArray(document.failed)
  ) {
    throw new Error(`do-harness verify returned an unrecognized report: ${tail(JSON.stringify(document))}`)
  }
  for (const sensor of document.sensors) {
    if (!sensor || typeof sensor.name !== 'string' || typeof sensor.ok !== 'boolean') {
      throw new Error(`do-harness verify returned a malformed sensor entry: ${tail(JSON.stringify(sensor))}`)
    }
  }
  const passed = document.sensors.filter((sensor) => sensor.ok).map((sensor) => sensor.name)
  const duration = document.sensors.reduce(
    (total, sensor) => total + (Number.isFinite(sensor.duration_ms) ? sensor.duration_ms : 0),
    0,
  )
  return {
    state: document.ok ? 'green' : 'red',
    set: typeof document.signal_set === 'string' ? document.signal_set : requestedSet,
    failed: document.failed,
    passed,
    duration_ms: duration,
    vacuous: document.sensors.length === 0,
  }
}

/** Bounds a diagnostic string to a short tail for error messages. */
export function tail(text, lines = TAIL_LINES) {
  const all = String(text ?? '').split('\n')
  return all.slice(Math.max(0, all.length - lines)).join('\n').trim()
}

/** Runner bound to one binary/workspace; used by the tool and the gate. */
export class HarnessRunner {
  constructor({ ctx, binary, workspace }) {
    this.ctx = ctx
    this.binary = binary
    this.workspace = workspace
  }

  /** Streams the requested signal set (`verify --set`). */
  async run({ set, changed, signal }) {
    const args = ['verify', '--set', set, '--format', 'json']
    if (changed) {
      args.push('--changed')
    }
    const result = await runHarness({
      ctx: this.ctx,
      binary: this.binary,
      cwd: this.workspace,
      args,
      signal,
    })
    if (result.exitCode === 2) {
      throw new Error(
        `do-harness verify rejected the request (exit 2): ${tail(result.stderr) || tail(result.stdout)}`,
      )
    }
    const document = parseHarnessJson(result.stdout, 'verify')
    return toRunResult(document, set)
  }

  /** Cheap evidence freshness check (never executes sensors). */
  async status({ set, signal }) {
    const result = await runHarness({
      ctx: this.ctx,
      binary: this.binary,
      cwd: this.workspace,
      args: ['status', '--set', set, '--format', 'json'],
      graceMs: QUICK_GRACE_MS,
      signal,
    })
    if (result.exitCode === 2) {
      throw new Error(
        `do-harness status rejected the request (exit 2): ${tail(result.stderr) || tail(result.stdout)}`,
      )
    }
    const document = parseHarnessJson(result.stdout, 'status')
    return assertStatusDocument(document)
  }

  /** Change-aware applicability explanation (never executes sensors). */
  async explain({ set, changed, signal }) {
    const args = ['explain', '--set', set, '--format', 'json']
    if (changed) {
      args.push('--changed')
    }
    const result = await runHarness({
      ctx: this.ctx,
      binary: this.binary,
      cwd: this.workspace,
      args,
      graceMs: QUICK_GRACE_MS,
      signal,
    })
    if (result.exitCode !== 0) {
      throw new Error(`do-harness explain failed: ${tail(result.stderr) || tail(result.stdout)}`)
    }
    return parseHarnessJson(result.stdout, 'explain')
  }
}
