// Resolve the effective omlx base URL: when the myllm reverse proxy
// (a Tauri app that forwards /v1/* to the real omlx server and logs every
// request/response for its history view) is alive on its well-known local
// port, route traffic through it; otherwise talk to omlx directly. The probe
// hits {proxy origin}/health — the proxy forwards it to omlx itself, so a
// 2xx proves BOTH the proxy and the upstream are up (omlx down behind a live
// proxy yields 502 → direct). Framework-free so headless consumers (MCP
// wrappers, node scripts) can reuse it; pass a Tauri fetch as fetchFn from
// webview contexts to avoid CORS on localhost.

export const DIRECT_OMLX_BASE_URL = 'http://127.0.0.1:8000/v1'
export const PROXY_OMLX_BASE_URL = 'http://127.0.0.1:8088/v1'

const DEFAULT_TIMEOUT_MS = 400
const DEFAULT_TTL_MS = 12_000

/**
 * Is this URL the default local omlx server — the only target eligible for
 * the proxy override? A user who deliberately pointed an app at another
 * host/port must never be silently rerouted (the proxy's upstream is
 * hardwired to the local :8000).
 * @param {string} url candidate base URL
 * @returns {boolean} true for 127.0.0.1:8000 / localhost:8000, false otherwise (incl. parse errors)
 */
export function isDirectOmlxUrl(url) {
  try {
    const { host } = new URL(url)
    return host === '127.0.0.1:8000' || host === 'localhost:8000'
  }
  catch {
    return false
  }
}

/**
 * One-shot probe: GET {proxy origin}/health with a short timeout. 2xx means
 * the proxy (and omlx behind it) is alive → use the proxy; anything else
 * (timeout, refused, 502) → use the direct URL.
 * @param {{ directUrl?: string, proxyUrl?: string, fetchFn?: typeof fetch, timeoutMs?: number }} [params] config
 * @returns {Promise<string>} the base URL to use
 */
export async function resolveOmlxBaseUrl({
  directUrl = DIRECT_OMLX_BASE_URL,
  proxyUrl = PROXY_OMLX_BASE_URL,
  fetchFn = fetch,
  timeoutMs = DEFAULT_TIMEOUT_MS,
} = {}) {
  try {
    const healthUrl = new URL('/health', proxyUrl).toString()
    const signal = typeof AbortSignal?.timeout === 'function' ? AbortSignal.timeout(timeoutMs) : undefined
    const response = await fetchFn(healthUrl, { signal })
    return response.ok ? proxyUrl : directUrl
  }
  catch {
    return directUrl
  }
}

// proxyUrl → { promise, expiresAt }. Caching the promise (not the value)
// dedupes concurrent probes: several composables firing loadEnv() at once
// trigger a single /health request.
const cache = new Map()

/**
 * Cached {@link resolveOmlxBaseUrl}: at most one probe per proxyUrl per TTL
 * window, so callers may resolve before every LLM call without paying the
 * probe latency each time, while still noticing the proxy starting/stopping
 * within one TTL.
 * @param {{ directUrl?: string, proxyUrl?: string, fetchFn?: typeof fetch, timeoutMs?: number, ttlMs?: number, now?: () => number }} [params] config
 * @returns {Promise<string>} the base URL to use
 */
export function resolveOmlxBaseUrlCached({ ttlMs = DEFAULT_TTL_MS, now = Date.now, ...probeOptions } = {}) {
  const proxyUrl = probeOptions.proxyUrl ?? PROXY_OMLX_BASE_URL
  const cached = cache.get(proxyUrl)
  if (cached && now() < cached.expiresAt) return cached.promise
  const promise = resolveOmlxBaseUrl(probeOptions)
  cache.set(proxyUrl, { promise, expiresAt: now() + ttlMs })
  return promise
}

/** Test hook: drop all cached probe results. */
export function __resetOmlxBaseUrlCache() {
  cache.clear()
}
