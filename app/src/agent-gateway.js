// Vanilla-JS equivalent of @7n/tauri-components/vue's useAgent() composable —
// this app has no bundler/Vue, so the core agent-kit is wired up by hand here
// instead. Core files are vendored under ./vendor/tauri-components-core/
// (source: @7n/tauri-components v0.8.0, https://github.com/nitra/tauri-components)
// because tauri.conf.json's frontendDist ("../src") only serves app/src — the
// package's real location under app/node_modules is unreachable from the webview.
import { createAgentKit } from "./vendor/tauri-components-core/agent-kit.js";
import { createOpenAiChat } from "./vendor/tauri-components-core/llm.js";
import { TOOLS } from "./tool/catalog.js";
import { createSystemPrompt } from "./tool/prompt.js";
import { tauriTransport } from "./tool/transport-tauri.js";
import { createTauriJournalStore, readOmlxConfig } from "./tool/journal-store-tauri.js";

const ACTOR = { kind: "human", id: "local" };
const DEFAULT_BASE_URL = "http://127.0.0.1:8000/v1";
const DEFAULT_MODEL = "gemma-4-e4b-it-OptiQ-4bit";

const journalStore = createTauriJournalStore();

const kit = createAgentKit({
  catalog: TOOLS,
  systemPrompt: createSystemPrompt(),
  transport: tauriTransport,
  journal: journalStore,
});

let cachedChatConfig = null;

async function resolveChatConfig() {
  if (cachedChatConfig) return cachedChatConfig;
  let cfg = {};
  try {
    cfg = (await readOmlxConfig()) ?? {};
  } catch {
    cfg = {};
  }
  cachedChatConfig = {
    baseUrl: cfg.baseUrl || DEFAULT_BASE_URL,
    model: cfg.model || DEFAULT_MODEL,
    apiKey: cfg.apiKey || "",
  };
  return cachedChatConfig;
}

function friendlyError(message) {
  const msg = String(message ?? "");
  if (/fetch|NetworkError|ECONNREFUSED|Failed to fetch/i.test(msg)) {
    return `Локальний AI-сервер (omlx) недоступний. Перевірте, чи він запущений (${cachedChatConfig?.baseUrl ?? DEFAULT_BASE_URL}).`;
  }
  if (/omlx 401|omlx 403/i.test(msg)) {
    return "omlx відхилив запит — перевірте API-ключ у ~/.omlx/settings.json.";
  }
  return msg || "Невідома помилка агента.";
}

async function withFriendlyFailure(result) {
  if (result.status !== "failed" || result.error) return result;
  try {
    const record = await journalStore.load(result.requestId);
    return { ...result, error: friendlyError(record?.error) };
  } catch {
    return { ...result, error: friendlyError(null) };
  }
}

export async function requestAgent(intent) {
  const { baseUrl, model, apiKey } = await resolveChatConfig();
  const chat = createOpenAiChat({ baseUrl, model, apiKey });
  const result = await kit.request({ intent, actor: ACTOR, chat });
  return withFriendlyFailure(result);
}

export async function respondAgent(requestId, message) {
  const { baseUrl, model, apiKey } = await resolveChatConfig();
  const chat = createOpenAiChat({ baseUrl, model, apiKey });
  const result = await kit.respond({ requestId, message, actor: ACTOR, chat });
  return withFriendlyFailure(result);
}

export async function approveAgent(requestId, approve) {
  return kit.approve({ requestId, approve });
}

// True when any tool call in this turn actually ran (regardless of tier) —
// used by main.js to decide whether to refresh the dashboard.
export function hadToolActivity(result) {
  return Array.isArray(result?.actions) && result.actions.length > 0;
}
