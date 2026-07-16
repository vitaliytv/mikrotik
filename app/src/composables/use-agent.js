import { useAgent as useAgentBase } from "@7n/tauri-components/vue";
import { TOOLS } from "../tool/catalog.js";
import { createSystemPrompt } from "../tool/prompt.js";

/**
 * @returns {object} the in-app agent gateway (baseUrl/model/apiKey refs, journal, request/respond/approve)
 */
export function useAgent() {
  return useAgentBase({
    catalog: TOOLS,
    systemPrompt: createSystemPrompt(),
    omlx: { storagePrefix: "mymikrotik", defaultModel: "gemma-4-e4b-it-OptiQ-4bit" },
  });
}
