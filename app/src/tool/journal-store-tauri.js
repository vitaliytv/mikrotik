// Thin wrapper over tauri-plugin-agent's journal_* commands (registered as
// plugin:agent|<command> — see @Users/vitalii/www/nitra/tauri-components
// /tauri-plugin-agent/src/lib.rs). Own implementation for the same reason as
// transport-tauri.js: no @tauri-apps/api package in this app.
export function createTauriJournalStore() {
  const { invoke } = window.__TAURI__.core;
  return {
    create: ({ intent, actor }) => invoke("plugin:agent|journal_create", { intent, actor }),
    load: (id) => invoke("plugin:agent|journal_load", { id }),
    update: (id, patch) => invoke("plugin:agent|journal_update", { id, patch }),
    list: () => invoke("plugin:agent|journal_list"),
  };
}

// Reads ~/.omlx/settings.json (base URL + api key) via the plugin's
// omlx_config command, so the UI doesn't need the user to configure the LLM
// endpoint manually.
export function readOmlxConfig() {
  return window.__TAURI__.core.invoke("plugin:agent|omlx_config");
}
