// List the models a local omlx (OpenAI-compatible) server has loaded, via
// GET {baseUrl}/models. Generic OpenAI shape: { data: [{ id }] }. Returns [] on
// any failure so a model picker degrades gracefully to manual entry.

/**
 * @param {{ baseUrl?: string, apiKey?: string, fetchFn?: typeof fetch, signal?: AbortSignal }} [params] config
 * @returns {Promise<string[]>} loaded model ids (empty on error)
 */
export async function listOmlxModels({ baseUrl, apiKey, fetchFn = fetch, signal } = {}) {
  if (!baseUrl) return []
  try {
    const response = await fetchFn(`${baseUrl}/models`, {
      headers: apiKey ? { authorization: `Bearer ${apiKey}` } : {},
      signal,
    })
    if (!response.ok) return []
    const data = await response.json()
    return Array.isArray(data?.data) ? data.data.map(m => m?.id).filter(Boolean) : []
  }
  catch {
    return []
  }
}

/**
 * Pick a model: the preferred one if loaded, else the first available, else ''.
 * @param {string[]} models loaded model ids
 * @param {string} [preferred] preferred id
 * @returns {string} chosen model id
 */
export function resolveModel(models, preferred) {
  if (preferred && models.includes(preferred)) return preferred
  return models[0] ?? ''
}
