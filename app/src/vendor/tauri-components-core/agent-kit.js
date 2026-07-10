import { handleApprove, handleRequest, handleRespond } from './agent-handler.js'
import { createDispatch } from './dispatch.js'
import { toolManifest } from './manifest.js'
import { classify, DEFAULT_ACTOR_TIERS, scopedManifest } from './scope.js'

// createAgentKit binds the generic agent machinery to ONE app's tool catalog.
// It is the single integration seam: an app supplies its catalog, a domain
// system prompt, a transport (Tauri invoke / CLI spawn / fetch), a journal store
// and optional grounding, and gets back request/respond/approve plus the derived
// manifest + scope helpers — all pre-bound, so callers never re-thread the
// catalog. The `chat` fn stays per-call (its config can change between requests).

/**
 * @param {object} config kit configuration
 * @param {object[]} config.catalog tool definitions (required)
 * @param {string|((ctx: object) => string)} [config.systemPrompt] domain system prompt (or builder from grounding ctx)
 * @param {(tool: object, input: object) => unknown} [config.transport] backend runner; omit to build the kit without dispatch (manifest/scope only)
 * @param {object} [config.journal] journal store { create, load, update, list }
 * @param {Record<string, number>} [config.actorTiers] max executable tier rank per actor kind
 * @param {{ tool: string, key?: string, fallback?: unknown }} [config.grounding] optional read tool to ground the prompt
 * @returns {{ dispatch: Function|null, classify: Function, scopedManifest: Function, toolManifest: Function, request: Function, respond: Function, approve: Function }} bound kit
 */
export function createAgentKit({ catalog, systemPrompt = '', transport, journal, actorTiers = DEFAULT_ACTOR_TIERS, grounding } = {}) {
  if (!Array.isArray(catalog)) throw new Error('createAgentKit: catalog (array) is required')

  const dispatch = transport ? createDispatch(catalog, transport) : null
  const buildSystem = typeof systemPrompt === 'function' ? systemPrompt : () => systemPrompt
  const toolsFor = actor => scopedManifest(catalog, actorTiers, actor)
  const gateFor = actor => name => classify(catalog, actorTiers, actor, name)

  return {
    dispatch,
    classify: (actor, name) => classify(catalog, actorTiers, actor, name),
    scopedManifest: actor => toolsFor(actor),
    toolManifest: allow => toolManifest(catalog, allow),
    request: ({ intent, actor, chat }) =>
      handleRequest({ intent, actor, chat, dispatch, journal, tools: toolsFor(actor), gate: gateFor(actor), buildSystem, grounding }),
    respond: ({ requestId, message, actor, chat }) =>
      handleRespond({ requestId, message, actor, chat, dispatch, journal, tools: toolsFor(actor), gate: gateFor(actor) }),
    approve: ({ requestId, approve }) =>
      handleApprove({ requestId, approve, dispatch, journal }),
  }
}
