// Computational, specific steering messages for the completion gate.
//
// Messages are built from the machine-readable status document, never from
// generic motivational prose, and always name the next concrete action.

let sequence = 0

/** Builds a plugin-sourced user message carried by `agent.steer`. */
export function userMessage(text) {
  sequence += 1
  return {
    id: `do-harness-${Date.now()}-${sequence}`,
    role: 'user',
    content: [{ type: 'text', text }],
    source: { kind: 'plugin', plugin: 'do-harness' },
  }
}

/** Builds the continuation prompt for a non-green status document. */
export function completionMessage(document) {
  const set = document.set ?? 'verification'
  const reason = document.reason ? ` (${document.reason})` : ''
  switch (document.state) {
    case 'stale': {
      const cause =
        document.reason === 'policy_changed'
          ? 'do-harness.toml changed after the last verification run'
          : 'the workspace changed after the last verification run'
      return userMessage(
        `Completion evidence for signal set "${set}" is stale because ${cause}. ` +
          `Run development_signals(action="run", set="${set}") before claiming completion.`,
      )
    }
    case 'red': {
      const failed = (document.evidence?.failed ?? []).join(', ')
      return userMessage(
        `Verification for signal set "${set}" is red.` +
          (failed ? ` Failed signals: ${failed}.` : '') +
          ' Repair the failures and rerun verification before claiming completion.',
      )
    }
    case 'missing':
    default:
      return userMessage(
        `No current verification evidence exists for signal set "${set}"${reason}. ` +
          `Run development_signals(action="run", set="${set}") before claiming completion.`,
      )
  }
}

/** Builds the actionable message for a gate infrastructure/config failure. */
export function infrastructureMessage(error) {
  return userMessage(
    `The do-harness completion gate could not evaluate verification evidence: ${error.message} ` +
      'Fix the do-harness configuration or binary resolution, then run verification.',
  )
}

/** Builds the final notice before the gate stops enforcing for one agent. */
export function exhaustedMessage(document, attempts) {
  const set = document?.set ?? 'verification'
  const failed = (document?.evidence?.failed ?? []).join(', ')
  return userMessage(
    `The do-harness completion gate is disabled for this agent after ${attempts} attempts with ` +
      `unchanged evidence for signal set "${set}"` +
      (failed ? ` (failed: ${failed})` : '') +
      '. Resolve the underlying failure manually; completion is not verified.',
  )
}
