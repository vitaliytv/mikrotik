import { useAcpAgent as useAcpAgentBase } from "@7n/tauri-components/vue";
import { CODEX_ACP_AGENT_PRESET } from "@7n/tauri-components";
import { homeDir } from "@tauri-apps/api/path";
import { TOOLS } from "../tool/catalog.js";

// No real "project" concept for this read-only WAN-monitor app — all actual
// state comes through the domain tool catalog (MCP bridge), not filesystem
// access — so cwd is just a sane default for the spawned agent CLIs, not a
// meaningful workspace root. Falls back to "." outside a real Tauri runtime
// (e.g. browser dev preview) so an unavailable home dir can't crash the
// whole module graph via an unhandled top-level await rejection.
const cwd = await homeDir().catch(() => ".");

/**
 * @returns {object} the in-app ACP agent gateway (agentKind/modelTier refs, journal, loadEnv/request/respond/approve)
 */
export function useAcpAgent() {
  return useAcpAgentBase({
    catalog: TOOLS,
    cwd,
    agents: {
      codex: CODEX_ACP_AGENT_PRESET,
      cursor: {
        command: "cursor",
        args: ["agent", "acp"],
        tiers: {
          MIN: { label: "GPT-5 Mini", args: ["--model", "gpt-5-mini"] },
          AVG: { label: "Grok 4.5", args: ["--model", "cursor-grok-4.5-high"] },
          MAX: { label: "Auto", args: ["--model", "auto"] },
        },
      },
      pi: {
        // pi-acp hardcodes its own spawn args (`pi --mode rpc --no-themes`) and
        // has no model/provider passthrough — model comes from pi's own
        // ~/.pi/agent/settings.json, so no per-tier override is possible here.
        command: "npx",
        args: ["-y", "pi-acp"],
      },
    },
  });
}
