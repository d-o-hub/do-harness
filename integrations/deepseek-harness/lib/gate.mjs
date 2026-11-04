// Optional strict completion gate backed by current verification evidence.
//
// On `agent/turn-stopping` (serial, awaited) the gate performs only the cheap
// `status` operation. Green evidence permits the stop; stale/red/missing
// evidence steers the agent with a computational, specific reason. Every
// non-green evaluation counts against one bounded budget, so alternating
// outcomes cannot extend the loop. When the budget is spent the gate rejects
// the next step: the host then closes the turn as `blocked` instead of
// reporting a successful completion the evidence does not support.

import {
  completionMessage,
  exhaustedInfrastructureMessage,
  exhaustedMessage,
  infrastructureMessage,
} from './messages.mjs'

/** Registers the gate listeners; returns a disposer for both. */
export function installCompletionGate({ ctx, settings, runner, logger }) {
  if (!settings.strictCompletion) {
    return () => false
  }
  const attemptsByAgent = new Map()
  // A blocked agent is rejected once at the next step boundary. The host's
  // only blocked outcome is a rejected `agent/pre-step`; stopping the steer
  // alone lets the loop close the turn as `completed`.
  const blockedAgents = new WeakSet()

  function block(agent, attempts, reason) {
    blockedAgents.add(agent)
    logger?.warn?.(
      `do-harness completion gate blocked the turn after ${attempts} attempts with ${reason}`,
    )
  }

  const disposeStopping = ctx.on('agent/turn-stopping', async (payload) => {
    const agent = payload && payload.agent
    if (!agent || typeof agent.steer !== 'function') {
      return
    }
    if (blockedAgents.has(agent)) {
      return
    }
    const key = agentKey(agent)
    const entry = attemptsByAgent.get(key) ?? { count: 0 }
    let document
    try {
      document = await runner.status({ set: settings.completionSignalSet, signal: payload.signal })
    } catch (error) {
      const attempts = entry.count + 1
      attemptsByAgent.set(key, { count: attempts })
      if (attempts > settings.maxContinuations) {
        block(agent, attempts, `repeated infrastructure errors: ${error.message}`)
        agent.steer(exhaustedInfrastructureMessage(error, attempts))
        return
      }
      agent.steer(infrastructureMessage(error))
      return
    }
    if (document.state === 'green') {
      attemptsByAgent.delete(key)
      return
    }
    const attempts = entry.count + 1
    attemptsByAgent.set(key, { count: attempts })
    if (attempts > settings.maxContinuations) {
      block(agent, attempts, `non-green ${document.state} results`)
      agent.steer(exhaustedMessage(document, attempts))
      return
    }
    agent.steer(completionMessage(document))
  })

  const disposePreStep = ctx.on('agent/pre-step', async (payload, next) => {
    if (!payload || !payload.agent || !blockedAgents.has(payload.agent)) {
      return next()
    }
    // One rejected step ends this turn; a later turn starts with a fresh
    // budget instead of leaving the agent permanently unable to run.
    blockedAgents.delete(payload.agent)
    attemptsByAgent.delete(agentKey(payload.agent))
    return { kind: 'reject' }
  })

  return () => {
    disposeStopping()
    disposePreStep()
    return true
  }
}

/** Stable per-agent key without requiring a specific Agent shape. */
function agentKey(agent) {
  if (typeof agent.id === 'string' && agent.id !== '') {
    return agent.id
  }
  return 'default'
}
