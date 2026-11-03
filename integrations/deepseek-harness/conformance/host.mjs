// Locates the pinned client, prepares a disposable host profile, and runs one
// observed session.
//
// The host is the real published CLI. Only the model provider is synthetic:
// plugin loading, the tool runtime, subprocess execution, the agent loop, and
// the stop boundary are the client's own code.

import { spawn } from 'node:child_process'
import { createHash } from 'node:crypto'
import { existsSync, readFileSync, readdirSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'

export const PROFILE_NAME = 'conformance'
export const CLIENT_ID = 'deepseek-harness'

/** Returns the pinned client version installed under `hostDir`, or throws. */
export function readClientVersion(hostDir) {
  const manifest = join(hostDir, 'node_modules', '@deepseek-ai', 'dsh', 'package.json')
  if (!existsSync(manifest)) {
    throw new Error(`pinned client not installed: ${manifest}`)
  }
  return JSON.parse(readFileSync(manifest, 'utf8')).version
}

/** Hashes the adapter sources so a result names exact content, not a branch. */
export function adapterDigest(bundleDir) {
  const files = ['package.json', 'index.js', 'cordis.patch.yml']
  for (const name of readdirSync(join(bundleDir, 'lib')).sort()) {
    files.push(join('lib', name))
  }
  const hash = createHash('sha256')
  for (const file of files.sort()) {
    hash.update(file)
    hash.update('\0')
    hash.update(readFileSync(join(bundleDir, file)))
    hash.update('\0')
  }
  return `sha256:${hash.digest('hex')}`
}

/** Reads the adapter's own name and version from its manifest. */
export function readAdapterIdentity(bundleDir) {
  const manifest = JSON.parse(readFileSync(join(bundleDir, 'package.json'), 'utf8'))
  return { name: manifest.name, version: manifest.version }
}

/** Creates the disposable profile once per run directory. */
export async function ensureProfile({ dsh, home, cwd }) {
  const manifest = join(home, 'profiles', PROFILE_NAME, 'package.json')
  if (existsSync(manifest)) {
    return
  }
  const outcome = await runProcess({
    argv: [dsh, PROFILE_NAME, '--from-default-profile', 'headless'],
    cwd,
    env: { DSH_HOME: home },
  })
  // The CLI prints a "task is required" notice and still writes the profile;
  // the manifest is the observable contract, not the exit code.
  if (!existsSync(manifest)) {
    throw new Error(`could not create the host profile: ${outcome.stderr.slice(-400)}`)
  }
}

/** Writes the overlay that disables the paid provider and mounts the bundle. */
export function writePatch({
  path,
  providerPlugin,
  bundle,
  binary,
  workspace,
  maxContinuations,
  completionSignalSet = 'verification',
  strictCompletion = true,
}) {
  const config = [
    `        binary: ${binary}`,
    `        workspace: ${workspace}`,
    `        completionSignalSet: ${completionSignalSet}`,
    `        strictCompletion: ${strictCompletion}`,
    `        maxContinuations: ${maxContinuations}`,
  ]
  writeFileSync(
    path,
    [
      '- id: llm-deepseek',
      '  disabled: true',
      '- id: agent-default-model',
      '  config:',
      '    provider: scripted',
      '    model: scripted-1',
      '- insert:',
      `    - name: ${providerPlugin}`,
      '      config: {}',
      `    - name: ${bundle}`,
      '      config:',
      ...config,
      '',
    ].join('\n'),
  )
}

/** Runs one session and returns its process outcome plus recorded events. */
export async function runSession({ dsh, home, cwd, patch, script, out, timeoutMs = 180_000 }) {
  const outcome = await runProcess({
    argv: [dsh, PROFILE_NAME, '--patch', patch, 'start'],
    cwd,
    env: {
      DSH_HOME: home,
      CONFORMANCE_OUT: out,
      CONFORMANCE_SCRIPT: JSON.stringify(script),
      CONFORMANCE_WORKSPACE: cwd,
    },
    timeoutMs,
  })
  const events = existsSync(out)
    ? readFileSync(out, 'utf8')
        .split('\n')
        .filter((line) => line.trim() !== '')
        .map((line) => JSON.parse(line))
    : []
  return { ...outcome, events }
}

function runProcess({ argv, cwd, env, timeoutMs = 180_000 }) {
  return new Promise((resolve) => {
    const child = spawn(argv[0], argv.slice(1), {
      cwd,
      env: { ...process.env, ...env },
      stdio: ['ignore', 'pipe', 'pipe'],
    })
    let stdout = ''
    let stderr = ''
    const timer = setTimeout(() => child.kill('SIGKILL'), timeoutMs)
    child.stdout.on('data', (chunk) => {
      stdout += chunk
    })
    child.stderr.on('data', (chunk) => {
      stderr += chunk
    })
    child.on('error', (error) => {
      clearTimeout(timer)
      resolve({ exitCode: null, signal: null, stdout, stderr: `${stderr}${error.message}`, timedOut: false })
    })
    child.on('close', (code, signal) => {
      clearTimeout(timer)
      resolve({
        exitCode: code,
        signal,
        stdout,
        stderr,
        timedOut: code === null && signal === 'SIGKILL',
      })
    })
  })
}
