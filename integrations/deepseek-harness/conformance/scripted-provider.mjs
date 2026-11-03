// Deterministic, offline model provider and probe tools for the real-host
// conformance harness.
//
// This plugin is test code loaded beside the bundle under test. It replaces the
// paid model with a scripted stream, registers a probe tool that mutates the
// disposable repository through the host's real tool runtime, and records what
// the host did at each stop boundary. Nothing here is shipped to adopters.

import { appendFileSync } from 'node:fs'

export const name = 'do-harness-conformance-probe'

let currentAgent

export function apply(ctx) {
  const record = (event) => appendFileSync(process.env.CONFORMANCE_OUT, `${JSON.stringify(event)}\n`)
  const script = JSON.parse(process.env.CONFORMANCE_SCRIPT ?? '[{"finish":"done"}]')
  const workspace = process.env.CONFORMANCE_WORKSPACE
  let calls = 0

  ctx.on('agent/created', (payload) => {
    currentAgent = payload.agent
    const tools = ctx.get('tools')
    const names =
      tools && typeof tools.schemas === 'function'
        ? tools.schemas(payload.agent).map((schema) => schema.name)
        : []
    record({ event: 'agent-created', tools: names })
  })
  ctx.on('agent/turn-stopping', (payload) => record({ event: 'turn-stopping', turn: payload.turn }))
  ctx.on('agent/status', (payload) => record({ event: 'agent-status', status: payload.status }))
  ctx.on('agent/error', (payload) => record({ event: 'agent-error', message: String(payload.error) }))

  ctx.inject(['llm', 'tools'], (injected) => {
    injected.tools.register({
      name: 'conformance_probe',
      description: 'Mutate the disposable conformance repository.',
      parameters: {
        type: 'object',
        properties: {
          action: {
            type: 'string',
            description: 'One of edit-source, edit-policy, noop.',
          },
        },
        required: ['action'],
        additionalProperties: false,
      },
      output: { schema: { type: 'string' }, render: (_args, value) => [{ type: 'text', text: value }] },
      async execute(args) {
        if (args.action === 'edit-source') {
          appendFileSync(`${workspace}/src.txt`, 'edited\n')
          return 'probe:source-edited'
        }
        if (args.action === 'edit-policy') {
          appendFileSync(`${workspace}/do-harness.toml`, '\n# policy edit\n')
          return 'probe:policy-edited'
        }
        return 'probe:noop'
      },
    })

    injected.llm.registerAdapter(['scripted'], {
      providerInfo: (provider) => ({ id: provider, name: 'scripted-offline' }),
      providerRetryPolicy: () => undefined,
      imageRequestPricing: () => undefined,
      resolveModel: (provider, model) => Promise.resolve({ provider, id: model, name: model }),
      listModels: () => Promise.resolve([]),
      async prepareCall(provider, model) {
        return { model: { provider, id: model, name: model }, stream: (options) => streamFor(options) }
      },
    })

    async function* streamFor(options) {
      const messages = Array.isArray(options?.messages) ? options.messages : []
      if (messages.some((message) => message?.source?.kind === 'dsh-session-title-llm')) {
        yield* text('conformance session')
        return
      }
      calls += 1
      record({
        event: 'model-call',
        call: calls,
        harness_messages: messages
          .filter((message) => message?.source?.kind === 'do-harness')
          .map((message) => textOf(message).slice(0, 200)),
      })
      const step = script[Math.min(calls - 1, script.length - 1)] ?? { finish: 'done' }
      if (step.cancel !== undefined) {
        record({ event: 'cancel-requested', cause: step.cancel })
        currentAgent?.cancel({ kind: 'user' })
        yield* text('cancelled')
        return
      }
      if (step.tool !== undefined) {
        yield* text(`calling ${step.tool.name}`)
        yield { type: 'block-start', index: 1, blockType: 'tool-call' }
        yield {
          type: 'tool-call-delta',
          index: 1,
          id: `conformance-call-${calls}`,
          name: step.tool.name,
          argumentsDelta: JSON.stringify(step.tool.arguments ?? {}),
        }
        yield {
          type: 'block-end',
          index: 1,
          block: {
            type: 'tool-call',
            id: `conformance-call-${calls}`,
            name: step.tool.name,
            arguments: JSON.stringify(step.tool.arguments ?? {}),
          },
        }
        yield { type: 'usage', usage: { inputTokens: 1, outputTokens: 1 } }
        yield { type: 'finish', reason: { kind: 'tool-calls' } }
        return
      }
      yield* text(step.finish ?? 'done')
    }

    async function* text(value) {
      yield { type: 'block-start', index: 0, blockType: 'text' }
      yield { type: 'text-delta', index: 0, text: value }
      yield { type: 'block-end', index: 0, block: { type: 'text', text: value } }
      yield { type: 'usage', usage: { inputTokens: 1, outputTokens: 1 } }
      yield { type: 'finish', reason: { kind: 'stop' } }
    }
  })
}

function textOf(message) {
  return (Array.isArray(message.content) ? message.content : [])
    .map((block) => (block.type === 'text' ? block.text : `<${block.type}>`))
    .join(' ')
}
