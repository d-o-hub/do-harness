// The model-facing `development_signals` capability.
//
// One typed tool over the Rust-owned semantics; it never exposes raw argv.
// `run` executes a signal set, `status` inspects evidence freshness, `explain`
// reports change-aware selection, and `context` returns bounded resume context —
// all by fixed argv templates.

export const TOOL_NAME = 'development_signals'

/** Which actions are available to the model. */
const ACTIONS = ['run', 'status', 'explain', 'context']
/** Runner parameters per action; anything else is rejected. */
const ACTION_FIELDS = {
  run: ['set', 'changed', 'since', 'task', 'reuse'],
  status: ['set'],
  explain: ['set', 'changed', 'since'],
  context: ['task'],
}

/** Builds the raw JSON-Schema ToolDefinition registered into ctx.tools. */
export function defineDevelopmentSignalsTool({ runner, settings }) {
  return {
    name: TOOL_NAME,
    description:
      'Inspect and produce do-harness development-signal evidence for this repository. ' +
      'action="run" executes a signal set (e.g. feedback during edits, verification before ' +
      'completion); action="status" cheaply reports evidence freshness ' +
      '(green|red|stale|missing|running) without running sensors; action="explain" ' +
      'reports which sensors apply to the current change and why; action="context" ' +
      'returns bounded resume context for a task. Do not disable or weaken signals.',
    parameters: {
      type: 'object',
      additionalProperties: false,
      properties: {
        action: {
          type: 'string',
          enum: ACTIONS,
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
        since: {
          type: 'string',
          description:
            'For run/explain: restrict to sensors applicable to the committed range ' +
            'merge-base(<ref>, HEAD)..HEAD.',
        },
        task: {
          type: 'integer',
          minimum: 1,
          description:
            'Task id: required for action="context"; optional for action="run" to scope ' +
            'recorded beats (and required when reuse=true).',
        },
        reuse: {
          type: 'boolean',
          description:
            'For action="run" only, with task: reuse recorded passing beats for sensors whose ' +
            'declared inputs are unchanged (--unchanged=skip) instead of executing them.',
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
      const completionSet = settings.completionSignalSet
      if (input.action === 'run') {
        const set = input.set ?? settings.defaultSignalSet
        const completion = set === completionSet
        if (
          completion &&
          (input.changed || input.since !== undefined || input.reuse)
        ) {
          throw new Error(
            'development_signals: the completion set does not accept changed/since/reuse; ' +
              'verify strictly and check status',
          )
        }
        const value = await runner.run({
          set,
          changed: input.changed,
          since: input.since,
          task: input.task,
          reuse: input.reuse,
          signal,
          completion,
        })
        const complete = value.state === 'green' && !value.vacuous && value.evidence?.state === 'green'
        return {
          action: 'run',
          ...value,
          complete,
          next_action: complete ? null : nextActionForRun(value),
        }
      }
      if (input.action === 'status') {
        const set = input.set ?? completionSet
        const document = await runner.status({ set, signal })
        return { action: 'status', ...document, next_action: nextActionForStatus(document) }
      }
      if (input.action === 'explain') {
        const set = input.set ?? completionSet
        const report = await runner.explain({
          set,
          changed: input.changed,
          since: input.since,
          signal,
        })
        return { action: 'explain', ...report, next_action: nextActionForExplain(report) }
      }
      const value = await runner.context({ task: input.task, signal })
      const ok = value.outcome === 'ok'
      const next = ok && value.document?.next_action?.kind === 'verify' ? 'verify' : null
      return {
        action: 'context',
        state: ok ? 'ok' : 'unavailable',
        task: input.task,
        document: value.document,
        reason: value.reason,
        diagnostics: value.diagnostics,
        next_action: next,
      }
    },
  }
}

/** The single next action for a non-complete run outcome. */
function nextActionForRun(value) {
  if (value.vacuous) {
    return 'explain'
  }
  return 'verify'
}

/** The single next action for a status document. */
function nextActionForStatus(document) {
  switch (document.state) {
    case 'green':
      return null
    case 'running':
      return 'status'
    default:
      return 'verify'
  }
}

/** The single next action for an explanation report. */
function nextActionForExplain(report) {
  return report.selected && report.selected.length > 0 ? 'verify' : null
}

/** Validates model arguments the raw JSON Schema cannot express. */
export function validateArgs(raw) {
  if (raw === null || typeof raw !== 'object' || Array.isArray(raw)) {
    throw new Error('development_signals: arguments must be an object')
  }
  const { action } = raw
  if (!ACTIONS.includes(action)) {
    throw new Error(
      `development_signals: action must be one of ${ACTIONS.join(', ')} (got ${JSON.stringify(action)})`,
    )
  }
  const allowed = ACTION_FIELDS[action]
  for (const key of Object.keys(raw)) {
    if (key !== 'action' && !allowed.includes(key)) {
      throw new Error(`development_signals: ${action} does not accept field ${key}`)
    }
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
  let since
  if (raw.since !== undefined) {
    if (typeof raw.since !== 'string' || raw.since.trim() === '') {
      throw new Error('development_signals: since must be a non-empty ref when provided')
    }
    since = raw.since.trim()
  }
  let task
  if (raw.task !== undefined) {
    if (!Number.isSafeInteger(raw.task) || raw.task < 1) {
      throw new Error('development_signals: task must be a positive safe integer when provided')
    }
    task = raw.task
  }
  if (raw.reuse !== undefined && typeof raw.reuse !== 'boolean') {
    throw new Error('development_signals: reuse must be a boolean when provided')
  }
  if (action === 'context' && task === undefined) {
    throw new Error('development_signals: context requires a positive integer task')
  }
  if (action === 'run' && raw.reuse === true && task === undefined) {
    throw new Error('development_signals: reuse requires a task scope')
  }
  return {
    action,
    set,
    changed: raw.changed === true,
    since,
    task,
    reuse: raw.reuse === true,
  }
}
