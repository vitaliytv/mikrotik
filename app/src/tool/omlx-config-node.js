// Mirrors tauri-plugin-agent's omlx_config Rust command (reads the same
// ~/.omlx/settings.json), so the Node MCP/CLI side picks up the same
// baseUrl/apiKey as the in-app agent without manual configuration. Env vars
// take priority, matching nitra/task's bin/task.mjs convention.
import { readFileSync } from "node:fs";
import os from "node:os";
import path from "node:path";

const DEFAULT_BASE_URL = "http://127.0.0.1:8000/v1";
const DEFAULT_MODEL = "gemma-4-e4b-it-OptiQ-4bit";

function fromSettingsFile() {
  try {
    const raw = readFileSync(path.join(os.homedir(), ".omlx", "settings.json"), "utf8");
    const json = JSON.parse(raw);
    const host = json?.server?.host;
    const port = json?.server?.port;
    return {
      baseUrl: host && port ? `http://${host}:${port}/v1` : null,
      apiKey: json?.auth?.api_key || null,
    };
  } catch {
    return { baseUrl: null, apiKey: null };
  }
}

export function resolveOmlxConfig() {
  const fromFile = fromSettingsFile();
  return {
    baseUrl: process.env.OMLX_BASE_URL || fromFile.baseUrl || DEFAULT_BASE_URL,
    model: process.env.OMLX_MODEL || DEFAULT_MODEL,
    apiKey: process.env.OMLX_API_KEY || fromFile.apiKey || undefined,
  };
}
