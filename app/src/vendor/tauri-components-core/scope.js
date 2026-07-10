import { toolManifest } from './manifest.js'
import { getTool } from './tools.js'

// Trust scope. Each tool has a tier; each actor kind has a max tier it may
// EXECUTE directly. Above that:
//   - agent + destructive → 'approval' (request goes to the human via the journal);
//   - otherwise           → 'deny'.
// Humans (UI, at the keyboard) execute everything directly.

const TIER_RANK = { read: 0, write: 1, destructive: 2 }
export const DEFAULT_ACTOR_TIERS = { human: 2, agent: 1 }

/**
 * Classify a tool call for an actor.
 * @param {object[]} catalog tool definitions
 * @param {Record<string, number>|undefined} actorTiers max executable tier rank per actor kind
 * @param {{ kind?: string }} actor caller identity
 * @param {string} toolName tool name
 * @returns {'allow'|'approval'|'deny'} decision
 */
export function classify(catalog, actorTiers, actor, toolName) {
  const tiers = actorTiers ?? DEFAULT_ACTOR_TIERS
  const tool = getTool(catalog, toolName)
  if (!tool) return 'deny'
  const rank = TIER_RANK[tool.tier] ?? Number.POSITIVE_INFINITY
  const max = tiers[actor?.kind] ?? 0
  if (rank <= max) return 'allow'
  if (tool.tier === 'destructive' && actor?.kind === 'agent') return 'approval'
  return 'deny'
}

/**
 * LLM tool manifest visible to the actor (everything it may run OR request approval for).
 * @param {object[]} catalog tool definitions
 * @param {Record<string, number>|undefined} actorTiers max executable tier rank per actor kind
 * @param {{ kind?: string }} actor caller identity
 * @returns {object[]} OpenAI tools array
 */
export function scopedManifest(catalog, actorTiers, actor) {
  return toolManifest(catalog, tool => classify(catalog, actorTiers, actor, tool.name) !== 'deny')
}

/**
 * Tool names visible to the actor.
 * @param {object[]} catalog tool definitions
 * @param {Record<string, number>|undefined} actorTiers max executable tier rank per actor kind
 * @param {{ kind?: string }} actor caller identity
 * @returns {string[]} visible tool names
 */
export function scopedToolNames(catalog, actorTiers, actor) {
  return catalog.filter(tool => classify(catalog, actorTiers, actor, tool.name) !== 'deny').map(tool => tool.name)
}
