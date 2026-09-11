// The model-facing `development_signals` capability.
//
// One typed tool over the Rust-owned semantics; it never exposes raw argv.
// `run` executes a signal set, `status` inspects evidence freshness, and
// `explain` reports change-aware selection — all by fixed argv templates.

export const TOOL_NAME = 'development_signals'

/** Builds the raw JSON-Schema ToolDefinition registered into ctx.tools. */
export function defineDevelopmentSignalsTool({ runner, settings }) {
  return {
    name: TOOL_NAME,
    description:
      'Inspect and produce do-harness development-signal evidence for this repository. ' +
      'action="run" executes a signal set (e.g. feedback during edits, verification before ' +
      'completion); action="status" cheaply reports evidence freshness ' +
      '(green|red|stale|missing) without running sensors; action="explain" reports which ' +
      'sensors apply to the current change and why. Do not disable or weaken signals.',
    parameters: {
      type: 'object',
      additionalProperties: false,
      properties: {
        action: {
          type: 'string',
          enum: ['run', 'status', 'explain'],
          description: 'Operation to perform.',
        },
        set: {
          type: 'string',
          description:
            'Development signal set name (for example feedback, verification, release). ' +
            'Defaults to the bundle default for run and the completion set for status.',
        },
        changed: {
          type: 'boolean',
          description:
            'For run/explain: restrict to sensors applicable to the current working-tree change.',
        },
      },
      required: ['action'],
    },
    output: {
      schema: { type: 'object' },
      render: (_args, value) => [{ type: 'text', text: JSON.stringify(value, null, 2) }],
    },
    async execute(args, exec) {
      const input = validateArgs(args)
      const signal = exec ? exec.signal : undefined
      if (input.action === 'run') {
        const set = input.set ?? settings.defaultSignalSet
        return runner.run({ set, changed: input.changed, signal })
      }
      if (input.action === 'status') {
        const set = input.set ?? settings.completionSignalSet
        return runner.status({ set, signal })
      }
      const set = input.set ?? settings.completionSignalSet
      return runner.explain({ set, changed: input.changed, signal })
    },
  }
}

/** Validates model arguments the raw JSON Schema cannot express. */
export function validateArgs(raw) {
  if (raw === null || typeof raw !== 'object' || Array.isArray(raw)) {
    throw new Error('development_signals: arguments must be an object')
  }
  const { action } = raw
  if (action !== 'run' && action !== 'status' && action !== 'explain') {
    throw new Error(
      `development_signals: action must be run, status, or explain (got ${JSON.stringify(action)})`,
    )
  }
  let set
  if (raw.set !== undefined) {
    if (typeof raw.set !== 'string' || raw.set.trim() === '') {
      throw new Error('development_signals: set must be a non-empty string when provided')
    }
    set = raw.set.trim()
  }
  if (raw.changed !== undefined && typeof raw.changed !== 'boolean') {
    throw new Error('development_signals: changed must be a boolean when provided')
  }
  return { action, set, changed: raw.changed === true }
}
