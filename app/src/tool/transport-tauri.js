// Dispatches a catalog tool by invoking its bound Tauri command. Own
// implementation (not @7n/tauri-components/vue's transport) since this app has
// no bundler and no @tauri-apps/api package — it relies on the injected
// window.__TAURI__ global (tauri.conf.json: withGlobalTauri: true).
export function tauriTransport(tool, input) {
  return window.__TAURI__.core.invoke(tool.tauri, input ?? {});
}
