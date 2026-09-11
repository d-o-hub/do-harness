// DSH bundle entry point: mounts the do-harness development-signal tool and
// the optional strict completion gate.
//
// The plugin owns no verification policy: binary discovery, signal-set
// selection, applicability, evidence freshness, and explanations all live in
// the do-harness CLI. This module only translates DSH lifecycle into fixed
// do-harness invocations through ctx.subprocess and validates the JSON it
// receives.

import { probeBinary, resolveBinary, HarnessRunner } from './lib/harness.mjs'
import { defineDevelopmentSignalsTool } from './lib/tool.mjs'
import { installCompletionGate } from './lib/gate.mjs'

export const name = 'do-harness-signals'
export const inject = ['tools', 'subprocess']

/** Resolves and validates bundle configuration with documented defaults. */
export function normalizeConfig(config = {}) {
  if (config === null || typeof config !== 'object' || Array.isArray(config)) {
    throw new Error('do-harness: config must be an object')
  }
  const settings = {
    binary: optionalString(config.binary, 'binary'),
    workspace: optionalString(config.workspace, 'workspace') ?? process.cwd(),
    defaultSignalSet: optionalString(config.defaultSignalSet, 'defaultSignalSet') ?? 'feedback',
    completionSignalSet:
      optionalString(config.completionSignalSet, 'completionSignalSet') ?? 'verification',
    strictCompletion: optionalBoolean(config.strictCompletion, 'strictCompletion') ?? false,
    maxContinuations: optionalPositiveInteger(config.maxContinuations, 'maxContinuations') ?? 3,
  }
  if (!settings.workspace.startsWith('/') && !/^[A-Za-z]:[\\/]/.test(settings.workspace)) {
    throw new Error(
      `do-harness: workspace must be an absolute path (got ${JSON.stringify(settings.workspace)})`,
    )
  }
  return settings
}

/** Mounts the tool and (optionally) the strict completion gate. */
export async function apply(ctx, config = {}) {
  const settings = normalizeConfig(config)
  const binary = resolveBinary(settings)
  await probeBinary({ ctx, binary, cwd: settings.workspace })
  const runner = new HarnessRunner({ ctx, binary, workspace: settings.workspace })
  ctx.tools.register(defineDevelopmentSignalsTool({ runner, settings }))
  installCompletionGate({ ctx, settings, runner, logger: ctx.logger })
  ctx.logger?.info?.(
    `do-harness signals ready (binary=${binary}, workspace=${settings.workspace}, ` +
      `strictCompletion=${settings.strictCompletion}, ` +
      `completionSignalSet=${settings.completionSignalSet})`,
  )
}

function optionalString(value, field) {
  if (value === undefined || value === null) {
    return undefined
  }
  if (typeof value !== 'string' || value.trim() === '') {
    throw new Error(`do-harness: ${field} must be a non-empty string when provided`)
  }
  return value.trim()
}

function optionalBoolean(value, field) {
  if (value === undefined || value === null) {
    return undefined
  }
  if (typeof value !== 'boolean') {
    throw new Error(`do-harness: ${field} must be a boolean when provided`)
  }
  return value
}

function optionalPositiveInteger(value, field) {
  if (value === undefined || value === null) {
    return undefined
  }
  if (!Number.isInteger(value) || value < 1) {
    throw new Error(`do-harness: ${field} must be a positive integer when provided`)
  }
  return value
}
