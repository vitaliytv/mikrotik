// Catalog helpers. A catalog is the app-provided array of tool definitions
// (`{ tier, name, summary, input, tauri, validate? }`). The package treats it as
// data passed in — never a module-level singleton — so each consumer app keeps
// its own domain tool surface while sharing the agent machinery.

/**
 * Look up a tool by name in a catalog.
 * @param {object[]} catalog tool definitions
 * @param {string} name tool name
 * @returns {object|null} the tool definition, or null if unknown
 */
export function getTool(catalog, name) {
  return catalog.find(tool => tool.name === name) ?? null
}
