// Optional strict completion gate backed by current verification evidence.
//
// On `agent/turn-stopping` (serial, awaited) the gate performs only the cheap
// `status` operation. Green evidence permits the stop; stale/red/missing
// evidence steers the agent with a computational, specific reason. Repeated
// identical failures and infrastructure errors are bounded so the gate can
// never create an infinite continuation loop.

import {
  completionMessage,
  exhaustedMessage,
  infrastructureMessage,
} from './messages.mjs'

/** Registers the gate listener; returns the disposer from ctx.on. */
export function installCompletionGate({ ctx, settings, runner, logger }) {
  if (!settings.strictCompletion) {
    return () => false
  }
  const attemptsByAgent = new Map()
  return ctx.on('agent/turn-stopping', async (payload) => {
    const agent = payload && payload.agent
    if (!agent || typeof agent.steer !== 'function') {
      return
    }
    const key = agentKey(agent)
    const entry = attemptsByAgent.get(key) ?? {
      count: 0,
      signature: null,
      disabled: false,
    }
    if (entry.disabled) {
      return
    }
    let document
    try {
      document = await runner.status({ set: settings.completionSignalSet, signal: payload.signal })
    } catch (error) {
      attemptsByAgent.set(key, recordAttempt(entry, `error:${error.message}`))
      if (attemptsByAgent.get(key).count > settings.maxContinuations) {
        attemptsByAgent.get(key).disabled = true
        logger?.warn?.(
          `do-harness completion gate disabled after repeated infrastructure errors: ${error.message}`,
        )
        agent.steer(infrastructureMessage(error))
        return
      }
      agent.steer(infrastructureMessage(error))
      return
    }
    if (document.state === 'green') {
      attemptsByAgent.delete(key)
      return
    }
    const signature = [
      document.state,
      document.reason,
      ...(document.evidence?.failed ?? []),
    ].join(':')
    const next = recordAttempt(entry, signature)
    attemptsByAgent.set(key, next)
    if (next.count > settings.maxContinuations) {
      next.disabled = true
      logger?.warn?.(
        `do-harness completion gate disabled after ${next.count} unchanged ${document.state} results`,
      )
      agent.steer(exhaustedMessage(document, next.count))
      return
    }
    agent.steer(completionMessage(document))
  })
}

/** Tracks consecutive identical outcomes for one agent. */
function recordAttempt(entry, signature) {
  if (entry.signature === signature) {
    return { ...entry, count: entry.count + 1 }
  }
  return { ...entry, signature, count: 1 }
}

/** Stable per-agent key without requiring a specific Agent shape. */
function agentKey(agent) {
  if (typeof agent.id === 'string' && agent.id !== '') {
    return agent.id
  }
  return 'default'
}
