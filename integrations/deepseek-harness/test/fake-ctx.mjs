// Test double for the Cordis context seam.
//
// Unit tests script canned subprocess outcomes; e2e tests use `realSpawn`
// so the plugin's execution path (ctx.subprocess.spawn with an explicit
// argv) drives the actual do-harness binary without a shell.

import { spawn } from 'node:child_process'

/** Creates a fake Cordis ctx with tools/subprocess/on/logger services. */
export function createFakeCtx({ responses = [], realSpawn = false } = {}) {
  const registered = new Map()
  const listeners = new Map()
  const pending = [...responses]
  const ctx = {
    warnings: [],
    logger: {
      warn: (...parts) => {
        ctx.warnings.push(parts.join(' '))
      },
      info: () => {},
    },
    tools: {
      register(definition) {
        registered.set(definition.name, definition)
      },
    },
    subprocess: {
      spawn(spec) {
        if (realSpawn) {
          return realHandle(spec)
        }
        if (pending.length === 0) {
          throw new Error(`no canned subprocess response for: ${spec.argv.join(' ')}`)
        }
        return cannedHandle(pending.shift(), spec)
      },
    },
    on(event, listener) {
      const set = listeners.get(event) ?? new Set()
      set.add(listener)
      listeners.set(event, set)
      return () => set.delete(listener)
    },
  }
  ctx.registered = registered
  ctx.listeners = listeners
  ctx.tool = (name) => registered.get(name)
  ctx.emit = async (event, payload) => {
    for (const listener of [...(listeners.get(event) ?? [])]) {
      await listener(payload)
    }
  }
  ctx.disposeAll = () => {
    for (const set of listeners.values()) {
      set.clear()
    }
  }
  ctx.spawnedArgv = []
  const originalSpawn = ctx.subprocess.spawn
  ctx.subprocess.spawn = (spec) => {
    ctx.spawnedArgv.push(spec.argv)
    return originalSpawn(spec)
  }
  return ctx
}

/** A canned subprocess handle returning the scripted outcome. */
function cannedHandle({ exitCode = 0, signal = null, stdout = '', stderr = '' }, spec) {
  if (spec.argv.join(' ').includes(' version ')) {
    // The load-time probe always succeeds unless the script says otherwise.
  }
  return {
    done: Promise.resolve({ exitCode, signal }),
    collected: {
      stdout: liveReader(() => stdout),
      stderr: liveReader(() => stderr),
    },
    terminate() {},
  }
}

/** A handle backed by a real child process (argv only, never a shell). */
function realHandle(spec) {
  const [program, ...args] = spec.argv
  const child = spawn(program, args, {
    cwd: spec.cwd,
    stdio: ['ignore', 'pipe', 'pipe'],
  })
  let stdout = ''
  let stderr = ''
  child.stdout.on('data', (chunk) => {
    stdout += chunk
  })
  child.stderr.on('data', (chunk) => {
    stderr += chunk
  })
  const done = new Promise((resolve, reject) => {
    child.on('error', reject)
    child.on('close', (code, signal) => resolve({ exitCode: code, signal }))
  })
  return {
    done,
    collected: {
      stdout: liveReader(() => stdout),
      stderr: liveReader(() => stderr),
    },
    terminate() {
      child.kill('SIGTERM')
    },
  }
}

/** Offset-based reader over a lazily read text buffer. */
function liveReader(getText) {
  return {
    readFrom(from = 0) {
      const text = getText()
      return { text: text.slice(from), nextOffset: text.length, lossy: false }
    },
  }
}

/** A steering agent double capturing steered messages. */
export function createFakeAgent(id = 'agent-1') {
  return {
    id,
    steered: [],
    steer(message) {
      this.steered.push(message)
    },
  }
}
