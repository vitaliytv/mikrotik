// Consumer-facing artifacts derived from a tool catalog. The LLM adapter (omlx —
// OpenAI-compatible MLX server) consumes the OpenAI function-calling shape. The
// catalog is passed in (see tools.js) so these stay app-agnostic.

/**
 * Convert a tool input spec into a JSON Schema object.
 * @param {Record<string, {type: string, required?: boolean, description?: string}>} input tool input spec
 * @returns {object} JSON Schema for the parameters object
 */
export function toJsonSchema(input) {
  const properties = {}
  const required = []
  for (const [key, spec] of Object.entries(input)) {
    properties[key] = spec.description ? { type: spec.type, description: spec.description } : { type: spec.type }
    if (spec.required) required.push(key)
  }
  return required.length ? { type: 'object', properties, required } : { type: 'object', properties }
}

/**
 * OpenAI function-calling tool definitions, optionally filtered (e.g. by scope).
 * @param {object[]} catalog tool definitions
 * @param {(tool: object) => boolean} [allow] predicate; default includes all tools
 * @returns {object[]} OpenAI `tools` array
 */
export function toolManifest(catalog, allow = () => true) {
  return catalog.filter(tool => allow(tool)).map(tool => ({
    type: 'function',
    function: {
      name: tool.name,
      description: tool.summary,
      parameters: toJsonSchema(tool.input),
    },
  }))
}

/**
 * Compact catalog listing (name + summary).
 * @param {object[]} catalog tool definitions
 * @returns {{name: string, summary: string}[]} tool list
 */
export function listTools(catalog) {
  return catalog.map(({ name, summary }) => ({ name, summary }))
}
